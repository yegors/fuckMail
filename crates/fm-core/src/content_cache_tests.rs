//! Export cache eviction, recently handed-out paths and immutable originals.
use super::*;
use std::time::{Duration, SystemTime};

#[test]
fn verified_read_preserves_bounds_hash_and_regular_file_checks() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_live(temp.path()).unwrap();
    let blobs = store.google_blob_store();
    let bytes = b"verified bytes\0\xff";
    let hash = blobs.stage_blob(bytes).unwrap();
    assert_eq!(
        blobs.read_verified(&hash, bytes.len() as u64).unwrap(),
        bytes
    );
    for size in [
        0,
        bytes.len() as u64 - 1,
        bytes.len() as u64 + 1,
        MAX_BLOB_BYTES + 1,
    ] {
        assert_eq!(
            blobs.read_verified(&hash, size).unwrap_err().code,
            "content_unavailable"
        );
    }
    assert!(blobs.read_verified("../store.sqlite3", 0).is_err());
    let empty = blobs.stage_blob(b"").unwrap();
    assert!(blobs.read_verified(&empty, 0).unwrap().is_empty());
    let path = temp.path().join("blobs").join(&hash);
    fs::write(&path, vec![0; bytes.len()]).unwrap();
    assert_eq!(
        blobs
            .read_verified(&hash, bytes.len() as u64)
            .unwrap_err()
            .code,
        "content_unavailable"
    );
    fs::remove_file(&path).unwrap();
    assert!(blobs.read_verified(&hash, bytes.len() as u64).is_err());
    fs::create_dir(&path).unwrap();
    assert!(blobs.read_verified(&hash, bytes.len() as u64).is_err());
}

#[test]
fn export_eviction_keeps_recent_paths_and_regenerates_removed_copies() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_live(temp.path()).unwrap();
    let mut copies = Vec::new();
    for number in 0..3u8 {
        let bytes = vec![number; 10];
        let hash = store.stage_blob(&bytes).unwrap();
        let path = store.export_attachment(&hash, 10, "file.bin").unwrap();
        if number < 2 {
            open_regular_with_write(&path, true)
                .unwrap()
                .set_modified(SystemTime::now() - Duration::from_secs(600 - number as u64))
                .unwrap();
        }
        copies.push((hash, path, bytes));
    }
    prune_exports(temp.path(), 10, EXPORT_GRACE).unwrap();
    assert!(!copies[0].1.exists());
    assert!(!copies[1].1.exists());
    assert!(copies[2].1.exists());
    for (hash, path, bytes) in &copies {
        assert_eq!(
            fs::read(store.verified_blob(hash, 10).unwrap()).unwrap(),
            *bytes
        );
        assert_eq!(
            store.export_attachment(hash, 10, "file.bin").unwrap(),
            *path
        );
        assert_eq!(fs::read(path).unwrap(), *bytes);
    }
    // A response can be in flight or a client can be about to open its path.
    // The byte budget does not race those recent consumers.
    prune_exports(temp.path(), 0, EXPORT_GRACE).unwrap();
    assert!(copies.iter().all(|(_, path, _)| path.exists()));
    prune_exports(temp.path(), 0, Duration::ZERO).unwrap();
    assert!(copies.iter().all(|(_, path, _)| !path.exists()));
}

#[test]
fn export_eviction_ignores_unrecognised_directories_and_user_downloads() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_live(temp.path()).unwrap();
    let hash = store.stage_blob(b"user data").unwrap();
    let path = store.export_attachment(&hash, 9, "note.txt").unwrap();
    let download = temp.path().join("saved-by-user.txt");
    fs::copy(path, &download).unwrap();
    let unknown = temp.path().join("exports").join("not-a-content-hash");
    fs::create_dir(&unknown).unwrap();
    fs::write(unknown.join("notes.txt"), b"keep").unwrap();
    prune_exports(temp.path(), 0, Duration::ZERO).unwrap();
    assert_eq!(fs::read(download).unwrap(), b"user data");
    assert_eq!(fs::read(unknown.join("notes.txt")).unwrap(), b"keep");
}
