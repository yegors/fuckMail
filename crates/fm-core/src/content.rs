//! Immutable retained content, published before any SQLite reference to it.
//!
//! Message sources, attachments and draft parts live under `<profile>/blobs`,
//! each named by the lowercase hex SHA-256 of its bytes. A blob is written to
//! a `.stage-*` file, flushed, then renamed into place, so a name is only ever
//! visible with complete bytes; SQL rows that reference it commit afterwards.
//! Every read re-verifies size and hash and refuses symlinks, Windows reparse
//! points and anything that is not a regular file. Files handed to other
//! applications are verified copies under `<profile>/exports`, never the
//! retained original.
use crate::{err, id, Result, Store};
use rusqlite::OptionalExtension;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// Largest single blob, in bytes (64 MiB). Larger content fails with
/// `content_too_large` before anything is written.
pub const MAX_BLOB_BYTES: u64 = 64 * 1024 * 1024;
static STAGE_SEQUENCE: AtomicU64 = AtomicU64::new(0);
// Serializes only the final rename, across every store in the process.
static BLOB_PUBLICATION: Mutex<()> = Mutex::new(());

/// A handle to a profile's blob directory, with no SQLite connection or
/// credentials.
///
/// It is cheap to clone and safe to use off the writer, so work such as
/// building a draft's MIME or reading a frozen upload can stage and verify
/// content while the [`Store`] keeps serving requests.
#[derive(Clone)]
pub struct BlobStore {
    pub(crate) profile: PathBuf,
}

impl BlobStore {
    /// Same as [`Store::stage_blob`].
    pub fn stage_blob(&self, bytes: &[u8]) -> Result<String> {
        stage_blob(&self.profile, bytes)
    }

    /// Same as [`Store::verified_blob`].
    pub fn verified_blob(&self, hash: &str, size: u64) -> Result<PathBuf> {
        verified_blob(&self.profile, hash, size)
    }

    pub(crate) fn export_attachment(&self, hash: &str, size: u64, name: &str) -> Result<PathBuf> {
        export_verified_attachment(&self.profile, hash, size, name)
    }
}

/// Deletes the blob `hash` if present. The caller must already have removed
/// every SQL reference to it; a missing file is not an error.
pub(crate) fn remove_unused_blob(profile: &Path, hash: &str) -> Result<()> {
    validate_hash(hash)?;
    let directory = blob_directory(profile, false)?;
    let path = directory.join(hash);
    match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            reject_non_regular(&metadata)?;
            fs::remove_file(path)?;
            sync_directory(&directory)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

impl Store {
    /// Returns a [`BlobStore`] for this profile that can be moved to another
    /// thread.
    pub fn google_blob_store(&self) -> BlobStore {
        BlobStore {
            profile: self.profile.clone(),
        }
    }
    /// Cheap list coverage check; content use still requires full hash verification.
    pub(crate) fn blob_present(&self, hash: &str) -> bool {
        if validate_hash(hash).is_err() {
            return false;
        }
        let Ok(directory) = blob_directory(&self.profile, false) else {
            return false;
        };
        fs::symlink_metadata(directory.join(hash)).is_ok_and(|metadata| {
            reject_non_regular(&metadata).is_ok() && metadata.len() <= MAX_BLOB_BYTES
        })
    }

    /// Writes and flushes `bytes`, publishes them as a blob and returns its
    /// SHA-256 hex name.
    ///
    /// Call it before the SQL transaction that references the hash. It is
    /// idempotent: existing content with the same hash is verified and reused,
    /// and fails with `content_unavailable` if that file is corrupt. Content
    /// over `MAX_BLOB_BYTES` fails with `content_too_large`. An interrupted
    /// write can leave a `.stage-*` orphan, which never counts as available.
    pub fn stage_blob(&self, bytes: &[u8]) -> Result<String> {
        stage_blob(&self.profile, bytes)
    }

    /// Returns the path of blob `hash` after checking it is a regular file of
    /// exactly `expected_size` bytes whose SHA-256 matches.
    ///
    /// Content is resolved only by a validated hash, never by a caller-supplied
    /// filename. Missing, altered or mismatched content fails with
    /// `content_unavailable`. The file is read in full on every call.
    pub fn verified_blob(&self, hash: &str, expected_size: u64) -> Result<PathBuf> {
        verified_blob(&self.profile, hash, expected_size)
    }

    pub(crate) fn export_attachment(&self, hash: &str, size: u64, name: &str) -> Result<PathBuf> {
        self.export_verified_attachment(hash, size, name)
    }
}

fn stage_blob(profile: &Path, bytes: &[u8]) -> Result<String> {
    if bytes.len() as u64 > MAX_BLOB_BYTES {
        return Err(err(
            "content_too_large",
            "Retained content exceeds the 64 MiB limit",
        ));
    }
    let hash = format!("{:x}", Sha256::digest(bytes));
    let directory = blob_directory(profile, true)?;
    let destination = directory.join(&hash);
    match fs::symlink_metadata(&destination) {
        Ok(_) => {
            verified_blob(profile, &hash, bytes.len() as u64)?;
            return Ok(hash);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let (temporary, mut file) = create_stage(&directory)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    // Background preparation may race another immutable publication. Hold
    // only the final name publication, never input hashing or file writes.
    {
        let _publication = BLOB_PUBLICATION.lock().map_err(|_| unavailable())?;
        match fs::symlink_metadata(&destination) {
            Ok(_) => fs::remove_file(&temporary)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::rename(&temporary, &destination)?;
                sync_directory(&directory)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    verified_blob(profile, &hash, bytes.len() as u64)?;
    Ok(hash)
}

/// Resolve content only by a validated hash, never by an untrusted filename.
fn verified_blob(profile: &Path, hash: &str, expected_size: u64) -> Result<PathBuf> {
    validate_hash(hash)?;
    if expected_size > MAX_BLOB_BYTES {
        return Err(unavailable());
    }
    let path = blob_directory(profile, false)
        .map_err(|_| unavailable())?
        .join(hash);
    verify_file(&path, hash, expected_size)?;
    Ok(path)
}

impl Store {
    fn export_verified_attachment(&self, hash: &str, size: u64, name: &str) -> Result<PathBuf> {
        export_verified_attachment(&self.profile, hash, size, name)
    }
}

fn export_verified_attachment(
    profile: &Path,
    hash: &str,
    size: u64,
    name: &str,
) -> Result<PathBuf> {
    let source = verified_blob(profile, hash, size)?;
    let exports = private_directory(profile, "exports", true)?;
    let directory = private_directory(&exports, hash, true)?;
    let destination = directory.join(safe_export_name(name));
    match fs::symlink_metadata(&destination) {
        Ok(_) => {
            verify_file(&destination, hash, size)?;
            return Ok(destination);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let (temporary, mut file) = create_stage(&directory)?;
    let copied = std::io::copy(&mut open_regular(&source)?.take(size + 1), &mut file)?;
    if copied != size {
        return Err(unavailable());
    }
    file.sync_all()?;
    drop(file);
    verify_file(&temporary, hash, size)?;
    {
        let _publication = BLOB_PUBLICATION.lock().map_err(|_| unavailable())?;
        match fs::symlink_metadata(&destination) {
            Ok(_) => fs::remove_file(&temporary)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::rename(&temporary, &destination)?;
                sync_directory(&directory)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    verify_file(&destination, hash, size)?;
    Ok(destination)
}
impl Store {
    /// Describes one attachment of `account`. When its content is `available`,
    /// the reply adds `path`, a verified copy under `exports/` named after the
    /// attachment, and `sha256`.
    ///
    /// Fails with `not_found` for an unknown account or attachment and with
    /// `content_unavailable` when the retained bytes no longer verify.
    pub(crate) fn attachment(&self, account: &str, attachment_id: &str) -> Result<Value> {
        id(account)?;
        id(attachment_id)?;
        let row = self.db.query_row(
            "SELECT message_id,name,mime_type,size,state,blob_hash FROM attachments WHERE account=?1 AND id=?2",
            [account, attachment_id],
            |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, i64>(3)?, row.get::<_, String>(4)?, row.get::<_, String>(5)?)),
        ).optional()?.ok_or_else(|| err("not_found", "Attachment does not exist in this account"))?;
        let (message_id, name, mime_type, size, state, hash) = row;
        let mut result = json!({"account":account,"id":attachment_id,"message_id":message_id,"name":name,"mime_type":mime_type,"size":size,"state":state});
        if state == "available" {
            let size = u64::try_from(size).map_err(|_| unavailable())?;
            // External editors receive a verified copy with a usable extension,
            // never the immutable retained original or a client-selected path.
            let path = self.export_attachment(&hash, size, &name)?;
            result["path"] = json!(path.to_str().ok_or_else(unavailable)?);
            result["sha256"] = json!(hash);
        }
        Ok(result)
    }
}

fn unavailable() -> crate::CoreError {
    err(
        "content_unavailable",
        "Retained content is missing, incomplete or failed verification",
    )
}

fn validate_hash(hash: &str) -> Result<()> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(unavailable());
    }
    Ok(())
}

fn blob_directory(profile: &Path, create: bool) -> Result<PathBuf> {
    private_directory(profile, "blobs", create)
}

/// Returns the canonical path of `parent/child`, optionally creating it with
/// owner-only permissions on Unix. Fails if either is a symlink, reparse point
/// or not a directory.
pub(crate) fn private_directory(parent: &Path, child: &str, create: bool) -> Result<PathBuf> {
    reject_non_directory(&fs::symlink_metadata(parent)?)?;
    let directory = parent.join(child);
    if create {
        #[cfg(unix)]
        let builder = {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = fs::DirBuilder::new();
            builder.mode(0o700);
            builder
        };
        #[cfg(not(unix))]
        let builder = fs::DirBuilder::new();
        match builder.create(&directory) {
            Ok(()) => sync_directory(parent)?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    reject_non_directory(&fs::symlink_metadata(&directory)?)?;
    Ok(fs::canonicalize(directory)?)
}

fn verify_file(path: &Path, hash: &str, expected_size: u64) -> Result<()> {
    validate_hash(hash)?;
    if expected_size > MAX_BLOB_BYTES {
        return Err(unavailable());
    }
    let mut file = open_regular(path).map_err(|_| unavailable())?;
    if file.metadata()?.len() != expected_size {
        return Err(unavailable());
    }
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let count = file.read(&mut buffer).map_err(|_| unavailable())?;
        if count == 0 {
            break;
        }
        total = total.checked_add(count as u64).ok_or_else(unavailable)?;
        if total > expected_size {
            return Err(unavailable());
        }
        digest.update(&buffer[..count]);
    }
    if total != expected_size || format!("{:x}", digest.finalize()) != hash {
        return Err(unavailable());
    }
    // Clients must remain inside the private profile permission boundary during
    // their subsequent open. Same-user software can edit an exported copy later.
    reject_non_regular(&fs::symlink_metadata(path).map_err(|_| unavailable())?)
}

fn safe_export_name(original: &str) -> String {
    // Treat provider metadata as a display name, never a filesystem path. Keep a
    // short ordinary extension for native application association and derive the
    // rest, including a name digest, to avoid platform aliases/collisions.
    let basename = original.rsplit(['/', '\\']).next().unwrap_or("");
    let (stem, extension) = match basename.rsplit_once('.') {
        Some((stem, extension))
            if !extension.is_empty()
                && extension.len() <= 16
                && extension.bytes().all(|byte| byte.is_ascii_alphanumeric()) =>
        {
            (stem, Some(extension))
        }
        _ => (basename, None),
    };
    let mut safe = String::new();
    for character in stem.chars() {
        let character = if character.is_alphanumeric() || matches!(character, '-' | '_' | ' ') {
            character
        } else {
            '_'
        };
        if safe.len() + character.len_utf8() > 96 {
            break;
        }
        safe.push(character);
    }
    let safe = safe.trim();
    let suffix = format!("{:x}", Sha256::digest(original.as_bytes()));
    let mut name = format!(
        "attachment-{}-{}",
        if safe.is_empty() { "file" } else { safe },
        &suffix[..12]
    );
    if let Some(extension) = extension {
        name.push('.');
        name.push_str(extension);
    }
    name
}

fn reject_non_directory(metadata: &fs::Metadata) -> Result<()> {
    if metadata.file_type().is_symlink() || !metadata.is_dir() || is_reparse_point(metadata) {
        return Err(unavailable());
    }
    Ok(())
}

fn reject_non_regular(metadata: &fs::Metadata) -> Result<()> {
    if metadata.file_type().is_symlink() || !metadata.is_file() || is_reparse_point(metadata) {
        return Err(unavailable());
    }
    Ok(())
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}
#[cfg(not(windows))]
fn is_reparse_point(_: &fs::Metadata) -> bool {
    false
}

/// Opens `path` for reading only if it is a regular file, checking both before
/// and after the open so a file swapped for a link in between is refused.
pub(crate) fn open_regular(path: &Path) -> Result<File> {
    reject_non_regular(&fs::symlink_metadata(path)?)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    let file = options.open(path)?;
    reject_non_regular(&file.metadata()?)?;
    Ok(file)
}

/// Creates a new, uniquely named `.stage-*` file in `directory` for writing.
/// The name combines process ID, clock and a counter; it never overwrites.
pub(crate) fn create_stage(directory: &Path) -> Result<(PathBuf, File)> {
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| {
            err(
                "storage_error",
                "System clock cannot name content staging file",
            )
        })?
        .as_nanos();
    for _ in 0..32 {
        let sequence = STAGE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let name = format!(".stage-{}-{epoch}-{sequence}", std::process::id());
        let path = directory.join(name);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(err(
        "storage_error",
        "Could not create a unique content staging file",
    ))
}

/// Flushes a directory entry change, such as a rename, to disk.
#[cfg(unix)]
pub(crate) fn sync_directory(directory: &Path) -> Result<()> {
    File::open(directory)?.sync_all()?;
    Ok(())
}
#[cfg(not(unix))]
pub(crate) fn sync_directory(_: &Path) -> Result<()> {
    // std exposes atomic same-volume rename and file flushing on Windows, but no
    // portable directory fsync, so the rename's durability rests on the
    // filesystem's own metadata journaling.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn store(profile: &Path) -> Store {
        fs::create_dir_all(profile).unwrap();
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(include_str!("fresh-schema.sql")).unwrap();
        db.execute_batch(crate::queries::STATE_LOG_SQL).unwrap();
        Store {
            mail_changes: crate::queries::MailChanges::install(&db),
            db,
            profile: profile.to_owned(),
            _lock: File::create(profile.join("content-test.lock")).unwrap(),
            projection_cache: Default::default(),
            coverage_cache: Default::default(),
            conversation_cache: Default::default(),
            planner_refreshed_at: Default::default(),
            state_scans: Default::default(),
        }
    }

    fn add_attachment(store: &Store, account: &str, hash: &str, size: i64, state: &str) {
        store
            .db
            .execute(
                "INSERT INTO accounts(id,name,email,provider) VALUES(?1,?1,?1,'synthetic')",
                [account],
            )
            .unwrap();
        store
            .db
            .execute(
                "INSERT INTO conversations(account,id,subject) VALUES(?1,'thread','subject')",
                [account],
            )
            .unwrap();
        store.db.execute("INSERT INTO messages(account,id,conversation_id,remote_id,sender_name,sender_email,recipient,subject,body,sent_at,unread,starred,inbox,sent,source_hash) VALUES(?1,'message','thread','message','Sender','sender@example.test','recipient@example.test','subject','body','2026-09-11T09:00:00Z',1,0,1,0,?2)", [account, hash]).unwrap();
        store.db.execute("INSERT INTO attachments VALUES(?1,'attachment','message','name.txt','text/plain',?2,?3,?4)", rusqlite::params![account,size,state,hash]).unwrap();
    }

    #[test]
    fn content_is_published_verified_and_idempotent() {
        let temp = tempfile::tempdir().unwrap();
        let store = store(&temp.path().join("profile"));
        let bytes = b"Exact authored source\n\0\xff";
        let hash = store.stage_blob(bytes).unwrap();
        assert!(store.blob_present(&hash));
        assert!(!store.blob_present("../store.sqlite3"));
        assert_eq!(hash, format!("{:x}", Sha256::digest(bytes)));
        let path = store.verified_blob(&hash, bytes.len() as u64).unwrap();
        assert!(path.is_absolute());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(store.stage_blob(bytes).unwrap(), hash);
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
        let empty = store.stage_blob(b"").unwrap();
        assert_eq!(
            fs::metadata(store.verified_blob(&empty, 0).unwrap())
                .unwrap()
                .len(),
            0
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(path.parent().unwrap())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
    }

    #[test]
    fn interrupted_orphans_are_never_available_or_removed_by_later_staging() {
        let temp = tempfile::tempdir().unwrap();
        let profile = temp.path().join("profile");
        let initial = store(&profile);
        let directory = blob_directory(&profile, true).unwrap();
        let orphan = directory.join(".stage-interrupted");
        fs::write(&orphan, b"partial").unwrap();
        drop(initial);
        let reopened = store(&profile);
        let hash = reopened.stage_blob(b"complete").unwrap();
        assert_eq!(
            fs::read(reopened.verified_blob(&hash, 8).unwrap()).unwrap(),
            b"complete"
        );
        assert_eq!(fs::read(&orphan).unwrap(), b"partial");
        assert!(reopened.verified_blob(".stage-interrupted", 7).is_err());
    }

    #[test]
    fn missing_corrupt_wrong_size_and_invalid_hash_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let store = store(&temp.path().join("profile"));
        let hash = store.stage_blob(b"original").unwrap();
        let path = store.verified_blob(&hash, 8).unwrap();
        assert_eq!(
            store.verified_blob(&hash, 9).unwrap_err().code,
            "content_unavailable"
        );
        fs::write(&path, b"modified").unwrap();
        assert_eq!(
            store.verified_blob(&hash, 8).unwrap_err().code,
            "content_unavailable"
        );
        assert!(store.stage_blob(b"original").is_err());
        assert_eq!(fs::read(&path).unwrap(), b"modified");
        fs::remove_file(path).unwrap();
        assert!(!store.blob_present(&hash));
        assert!(store.verified_blob(&hash, 8).is_err());
        for invalid in [
            "../store.sqlite3",
            "",
            &hash.to_uppercase(),
            &"f".repeat(63),
        ] {
            assert_eq!(
                store.verified_blob(invalid, 8).unwrap_err().code,
                "content_unavailable"
            );
        }
        assert!(store.verified_blob(&hash, MAX_BLOB_BYTES + 1).is_err());
    }

    #[test]
    fn attachment_lookup_is_account_scoped_and_checks_content_before_returning_path() {
        let temp = tempfile::tempdir().unwrap();
        let store = store(&temp.path().join("profile"));
        let hash = store.stage_blob(b"available bytes").unwrap();
        add_attachment(&store, "work", &hash, 15, "available");
        add_attachment(&store, "personal", &hash, 15, "partial");
        add_attachment(&store, "other", &hash, 15, "missing");
        let available = store.attachment("work", "attachment").unwrap();
        assert_eq!(available["sha256"], hash);
        assert!(available["path"].as_str().is_some());
        let exported = Path::new(available["path"].as_str().unwrap());
        assert_eq!(exported.extension().unwrap(), "txt");
        assert!(exported.starts_with(fs::canonicalize(store.profile.join("exports")).unwrap()));
        assert_ne!(exported, store.verified_blob(&hash, 15).unwrap());
        assert_eq!(
            store.attachment("work", "attachment").unwrap()["path"],
            available["path"]
        );
        assert!(store.attachment("personal", "attachment").unwrap()["path"].is_null());
        assert!(store.attachment("other", "attachment").unwrap()["path"].is_null());
        assert_eq!(
            store.attachment("unknown", "attachment").unwrap_err().code,
            "not_found"
        );
        assert_eq!(
            store.attachment("work", "unknown").unwrap_err().code,
            "not_found"
        );
        fs::write(available["path"].as_str().unwrap(), b"changed").unwrap();
        assert_eq!(
            fs::read(store.verified_blob(&hash, 15).unwrap()).unwrap(),
            b"available bytes"
        );
        assert_eq!(
            store.attachment("work", "attachment").unwrap_err().code,
            "content_unavailable"
        );
    }

    #[test]
    fn export_names_cannot_choose_paths_or_reserved_device_names() {
        let temp = tempfile::tempdir().unwrap();
        let store = store(&temp.path().join("profile"));
        let hash = store.stage_blob(b"attachment").unwrap();
        add_attachment(&store, "work", &hash, 10, "available");
        let sentinel = temp.path().join("outside.txt");
        fs::write(&sentinel, b"never replace me").unwrap();
        let names = [
            "../../outside.txt",
            "C:\\Windows\\outside.txt",
            "CON.txt",
            "AUX",
            "..",
            "/etc/passwd",
            "财务 — отчет.PDF",
            "same?.txt",
            "same*.txt",
        ];
        let mut paths = std::collections::BTreeSet::new();
        for name in names {
            store
                .db
                .execute(
                    "UPDATE attachments SET name=?1 WHERE account='work'",
                    [name],
                )
                .unwrap();
            let reply = store.attachment("work", "attachment").unwrap();
            assert_eq!(reply["name"], name);
            let path = PathBuf::from(reply["path"].as_str().unwrap());
            assert!(path.starts_with(fs::canonicalize(store.profile.join("exports")).unwrap()));
            assert!(path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("attachment-"));
            assert_eq!(fs::read(&path).unwrap(), b"attachment");
            assert!(paths.insert(path));
        }
        assert_eq!(fs::read(sentinel).unwrap(), b"never replace me");
        let long = format!("{}.pdf", "🦀".repeat(500));
        let path = store.export_attachment(&hash, 10, &long).unwrap();
        assert_eq!(path.extension().unwrap(), "pdf");
        assert!(path.file_name().unwrap().len() < 180);
    }

    #[cfg(unix)]
    #[test]
    fn exported_copy_and_export_directory_symlinks_are_rejected() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let store = store(&temp.path().join("profile"));
        let hash = store.stage_blob(b"original").unwrap();
        let path = store.export_attachment(&hash, 8, "notes.txt").unwrap();
        let outside = temp.path().join("outside.txt");
        fs::write(&outside, b"original").unwrap();
        fs::remove_file(&path).unwrap();
        symlink(&outside, &path).unwrap();
        assert!(store.export_attachment(&hash, 8, "notes.txt").is_err());
        fs::remove_file(path).unwrap();
        fs::remove_dir(store.profile.join("exports").join(&hash)).unwrap();
        fs::remove_dir(store.profile.join("exports")).unwrap();
        let directory = temp.path().join("outside-directory");
        fs::create_dir(&directory).unwrap();
        symlink(&directory, store.profile.join("exports")).unwrap();
        assert!(store.export_attachment(&hash, 8, "notes.txt").is_err());
        assert_eq!(fs::read_dir(directory).unwrap().count(), 0);
        assert_eq!(
            fs::read(store.verified_blob(&hash, 8).unwrap()).unwrap(),
            b"original"
        );
    }

    #[test]
    fn over_limit_content_is_rejected_before_creating_storage() {
        let temp = tempfile::tempdir().unwrap();
        let store = store(&temp.path().join("profile"));
        assert_eq!(
            store
                .stage_blob(&vec![0; MAX_BLOB_BYTES as usize + 1])
                .unwrap_err()
                .code,
            "content_too_large"
        );
        assert!(!store.profile.join("blobs").exists());
    }

    #[cfg(unix)]
    #[test]
    fn linked_blob_and_linked_content_directory_are_rejected() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let store = store(&temp.path().join("profile"));
        let hash = store.stage_blob(b"verified").unwrap();
        let path = store.verified_blob(&hash, 8).unwrap();
        let outside = temp.path().join("outside");
        fs::rename(&path, &outside).unwrap();
        symlink(&outside, &path).unwrap();
        assert!(!store.blob_present(&hash));
        assert!(store.verified_blob(&hash, 8).is_err());
        assert!(store.stage_blob(b"verified").is_err());
        fs::remove_file(&path).unwrap();
        fs::remove_dir(store.profile.join("blobs")).unwrap();
        let linked_directory = temp.path().join("external-blobs");
        fs::create_dir(&linked_directory).unwrap();
        symlink(&linked_directory, store.profile.join("blobs")).unwrap();
        assert!(store.stage_blob(b"verified").is_err());
        assert!(store.verified_blob(&hash, 8).is_err());
        assert_eq!(fs::read_dir(linked_directory).unwrap().count(), 0);
    }
}
