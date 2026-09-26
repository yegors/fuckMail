//! A production build only knows how to refuse an isolated verification profile.
//!
//! Builds with the `verification-google` feature replace Google with the
//! in-memory fixture in `fixture` and mark the profile they use. Every build
//! refuses to open a marked profile, so fixture data can never meet real
//! Google services on a later start.
use std::{io, path::Path};

pub(crate) const MARKER: &str = "verification-google-profile.json";

/// Even an unreadable file, directory or dangling link is a refusal marker.
/// Run this before opening the Store or constructing any provider service.
pub(crate) fn refuse_marked_profile(profile: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(profile.join(MARKER)) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        _ => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Google verification profiles cannot be opened by an ordinary or restarted engine",
        )),
    }
}

#[cfg(feature = "verification-google")]
mod fixture;
#[cfg(feature = "verification-google")]
pub(crate) use fixture::{claim_empty_profile, record_publication, unix_ms, Fixture};

#[cfg(test)]
mod isolation_tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn marker_refusal_precedes_store_and_applies_to_file_and_directory_markers() {
        let directory = tempfile::tempdir().unwrap();
        let profile = directory.path();
        assert!(refuse_marked_profile(profile).is_ok());
        std::fs::write(profile.join(MARKER), b"not trusted JSON").unwrap();
        assert!(refuse_marked_profile(profile).is_err());
        assert!(!profile.join("store.sqlite3").exists());
        std::fs::remove_file(profile.join(MARKER)).unwrap();
        std::fs::create_dir(profile.join(MARKER)).unwrap();
        assert!(refuse_marked_profile(profile).is_err());
    }

    #[tokio::test]
    async fn ordinary_startup_refuses_marked_profile_without_creating_discovery_or_store() {
        let directory = tempfile::tempdir().unwrap();
        let profile = fm_ipc::private_profile(&directory.path().join("profile")).unwrap();
        std::fs::write(profile.join(MARKER), b"{}").unwrap();
        let args = crate::Args::try_parse_from([
            "fm-engine",
            "--profile",
            profile.to_str().unwrap(),
            "--live-profile",
        ])
        .unwrap();
        assert!(crate::run(args).await.is_err());
        assert!(!profile.join("engine.json").exists());
        assert!(!profile.join("store.sqlite3").exists());
    }

    #[test]
    fn verification_option_is_absent_from_default_build_and_requires_explicit_live_profile() {
        assert!(crate::Args::try_parse_from(["fm-engine", "--verification-google"]).is_err());
        #[cfg(not(feature = "verification-google"))]
        assert!(crate::Args::try_parse_from([
            "fm-engine",
            "--verification-google",
            "--live-profile",
            "--profile",
            "unused"
        ])
        .is_err());
    }

    #[cfg(feature = "verification-google")]
    #[tokio::test]
    async fn isolated_feature_refuses_unmarked_normal_startup_before_profile_creation() {
        let directory = tempfile::tempdir().unwrap();
        let profile = directory.path().join("never-created");
        let args = crate::Args::try_parse_from([
            "fm-engine",
            "--profile",
            profile.to_str().unwrap(),
            "--live-profile",
        ])
        .unwrap();
        assert!(crate::run(args).await.is_err());
        assert!(!profile.exists());
    }
}
