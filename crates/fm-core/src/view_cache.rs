//! Bounded private cache for decoded message images and oversized HTML views.
//!
//! Images are PNGs produced by `fm_mail::image_view`, stored per account under
//! `<profile>/cache/images/<account-key>/<key>.png` so account removal can drop
//! them. The whole image cache stays under [`IMAGE_CACHE_LIMIT`]; the oldest
//! files go first. Only pixels are kept, never response headers.
use crate::content::{create_stage, private_directory, sync_directory};
use crate::{err, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

pub const IMAGE_CACHE_LIMIT: u64 = 256 * 1024 * 1024;
const HTML_FILES_KEPT: usize = 16;

/// One per profile; clones share the running size and the eviction lock.
#[derive(Clone)]
pub struct ViewCache {
    profile: PathBuf,
    /// Bytes in the image cache, or `u64::MAX` before the first scan.
    bytes: Arc<AtomicU64>,
    eviction: Arc<Mutex<()>>,
}

pub fn key(parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update(part.as_bytes());
        digest.update([0]);
    }
    format!("{:x}", digest.finalize())
}

impl ViewCache {
    pub fn new(profile: impl Into<PathBuf>) -> Self {
        Self {
            profile: profile.into(),
            bytes: Arc::new(AtomicU64::new(u64::MAX)),
            eviction: Arc::new(Mutex::new(())),
        }
    }

    fn images(&self, create: bool) -> Result<PathBuf> {
        let cache = private_directory(&self.profile, "cache", create)?;
        private_directory(&cache, "images", create)
    }

    fn account_dir(&self, account: &str, create: bool) -> Result<PathBuf> {
        private_directory(&self.images(create)?, &key(&[account])[..32], create)
    }

    /// A cached PNG and its size, touched so eviction keeps recently shown images.
    pub fn cached_image(&self, account: &str, key: &str) -> Option<(PathBuf, u32, u32)> {
        let path = self
            .account_dir(account, false)
            .ok()?
            .join(format!("{key}.png"));
        let (width, height) = png_size(&path)?;
        if let Ok(file) = fs::File::options().write(true).open(&path) {
            let _ = file.set_modified(SystemTime::now());
        }
        Some((path, width, height))
    }

    pub fn store_image(&self, account: &str, key: &str, png: &[u8]) -> Result<PathBuf> {
        let directory = self.account_dir(account, true)?;
        let destination = directory.join(format!("{key}.png"));
        let (temporary, mut file) = create_stage(&directory)?;
        file.write_all(png)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, &destination)?;
        sync_directory(&directory)?;
        self.account_bytes(png.len() as u64);
        Ok(destination)
    }

    fn account_bytes(&self, added: u64) {
        let _ = self
            .bytes
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| {
                (v != u64::MAX).then(|| v.saturating_add(added))
            });
        if self.bytes.load(Ordering::SeqCst) > IMAGE_CACHE_LIMIT {
            let _ = self.evict(IMAGE_CACHE_LIMIT / 10 * 9);
        }
    }

    /// Scan once per process, then keep a running total.
    pub fn image_bytes(&self) -> u64 {
        let known = self.bytes.load(Ordering::SeqCst);
        if known != u64::MAX {
            return known;
        }
        let total: u64 = self.files().iter().map(|(_, size, _)| size).sum();
        self.bytes.store(total, Ordering::SeqCst);
        if total > IMAGE_CACHE_LIMIT {
            let _ = self.evict(IMAGE_CACHE_LIMIT / 10 * 9);
        }
        self.bytes.load(Ordering::SeqCst)
    }

    fn files(&self) -> Vec<(PathBuf, u64, SystemTime)> {
        let mut out = Vec::new();
        let Ok(images) = self.images(false) else {
            return out;
        };
        for account in fs::read_dir(images).into_iter().flatten().flatten() {
            let Ok(kind) = account.file_type() else {
                continue;
            };
            if !kind.is_dir() {
                continue;
            }
            for entry in fs::read_dir(account.path()).into_iter().flatten().flatten() {
                let Ok(meta) = entry.metadata() else { continue };
                if meta.is_file() {
                    out.push((
                        entry.path(),
                        meta.len(),
                        meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                    ));
                }
            }
        }
        out
    }

    /// Delete the oldest images until the cache is at most `target` bytes.
    pub fn evict(&self, target: u64) -> Result<()> {
        let _guard = self
            .eviction
            .lock()
            .map_err(|_| err("storage_error", "Image cache is busy"))?;
        let mut files = self.files();
        let mut total: u64 = files.iter().map(|(_, size, _)| size).sum();
        files.sort_by_key(|(_, _, modified)| *modified);
        for (path, size, _) in files {
            if total <= target {
                break;
            }
            if fs::remove_file(&path).is_ok() {
                total = total.saturating_sub(size);
            }
        }
        self.bytes.store(total, Ordering::SeqCst);
        Ok(())
    }

    pub fn clear_images(&self) -> Result<()> {
        self.evict(0)
    }

    /// Account removal drops that account's images.
    pub fn remove_account(&self, account: &str) {
        if let Ok(directory) = self.account_dir(account, false) {
            let _ = fs::remove_dir_all(directory);
        }
        self.bytes.store(u64::MAX, Ordering::SeqCst);
    }

    /// An attachment of a message read live from Google, written so it can be
    /// previewed or saved. Keeps only the newest few files.
    pub fn remote_file(&self, name: &str, bytes: &[u8]) -> Result<PathBuf> {
        let cache = private_directory(&self.profile, "cache", true)?;
        let directory = private_directory(&cache, "remote", true)?;
        let destination = directory.join(name);
        if !destination.exists() {
            let (temporary, mut file) = create_stage(&directory)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, &destination)?;
        }
        prune(&directory, &destination);
        Ok(destination)
    }

    /// Sanitised HTML too large to send inline. Keeps only the newest few files.
    pub fn html_file(&self, name: &str, html: &str) -> Result<PathBuf> {
        let cache = private_directory(&self.profile, "cache", true)?;
        let directory = private_directory(&cache, "html", true)?;
        let destination = directory.join(format!("{name}.html"));
        if !destination.exists() {
            let (temporary, mut file) = create_stage(&directory)?;
            file.write_all(html.as_bytes())?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, &destination)?;
        }
        prune(&directory, &destination);
        Ok(destination)
    }
}

/// Keep the newest few files of a directory, and always `keep`.
fn prune(directory: &Path, keep: &Path) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut files: Vec<_> = entries
        .flatten()
        .filter_map(|e| Some((e.path(), e.metadata().ok()?.modified().ok()?)))
        .collect();
    if files.len() > HTML_FILES_KEPT {
        files.sort_by_key(|(_, modified)| *modified);
        for (path, _) in &files[..files.len() - HTML_FILES_KEPT] {
            if path != keep {
                let _ = fs::remove_file(path);
            }
        }
    }
}

/// Width and height from a PNG's IHDR, without decoding it.
fn png_size(path: &Path) -> Option<(u32, u32)> {
    use std::io::Read;
    let mut header = [0u8; 24];
    fs::File::open(path).ok()?.read_exact(&mut header).ok()?;
    if &header[..8] != b"\x89PNG\r\n\x1a\n" || &header[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(header[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(header[20..24].try_into().ok()?);
    Some((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        bytes.extend(width.to_be_bytes());
        bytes.extend(height.to_be_bytes());
        bytes.resize(4096, 0);
        bytes
    }

    #[test]
    fn images_are_cached_per_account_and_bounded() {
        let temp = tempfile::tempdir().unwrap();
        let cache = ViewCache::new(temp.path());
        assert!(cache.cached_image("a", "k1").is_none());
        let path = cache.store_image("a", "k1", &png(30, 20)).unwrap();
        assert!(path.starts_with(fs::canonicalize(temp.path()).unwrap()));
        let (_, w, h) = cache.cached_image("a", "k1").unwrap();
        assert_eq!((w, h), (30, 20));
        assert!(cache.cached_image("b", "k1").is_none());
        cache.store_image("b", "k2", &png(1, 1)).unwrap();
        cache.evict(4096).unwrap();
        assert_eq!(cache.files().len(), 1);
        cache.remove_account("a");
        cache.remove_account("b");
        assert!(cache.files().is_empty());
        let html = cache.html_file("v1", "<p>x</p>").unwrap();
        assert_eq!(fs::read_to_string(html).unwrap(), "<p>x</p>");
    }
}
