//! App-owned OAuth registration. User grants remain exclusively in the vault.
use super::*;
use std::path::PathBuf;

/// The client registration file shipped next to the engine executable.
pub(super) const APP_CLIENT_FILE: &str = "google-oauth-client.json";

/// Where this build finds its Google OAuth client registration.
#[derive(Default)]
pub(super) struct AppClient {
    // Captured once at engine startup; tests inject a path.
    path: Option<PathBuf>,
    /// The path was set explicitly, so a missing file is an error rather than
    /// a reason to fall back to the profile's recorded registration.
    required: bool,
}

impl AppClient {
    /// The packaged registration beside the executable, or the file named by
    /// `FM_GOOGLE_CLIENT_CONFIG`, which is then required.
    pub(super) fn installed() -> Self {
        if let Some(path) = std::env::var_os("FM_GOOGLE_CLIENT_CONFIG") {
            return Self::configured(PathBuf::from(path));
        }
        Self {
            path: std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(|dir| dir.join(APP_CLIENT_FILE))),
            required: false,
        }
    }

    /// A required registration at `path`.
    pub(super) fn configured(path: PathBuf) -> Self {
        Self {
            path: Some(path),
            required: true,
        }
    }

    /// Loads the registration to sign in with, returning it and its path.
    ///
    /// Tries `explicit`, then this build's file, then the path recorded by an
    /// existing account. Fails with `google_not_configured` when there is none,
    /// and `client_mismatch` when its client id differs from one the profile's
    /// accounts already use.
    pub(super) async fn load(
        &self,
        explicit: Option<String>,
        references: &[GoogleClientReference],
    ) -> Result<(OAuthClientConfig, String)> {
        let path = if let Some(path) = explicit {
            path
        } else if let Some(path) = &self.path {
            // Missing optional package registration can use an existing profile's
            // original setup. A present invalid file or explicit override fails.
            match tokio::fs::symlink_metadata(path).await {
                Ok(_) => path.to_str().ok_or_else(invalid_config)?.to_owned(),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound && !self.required => {
                    Self::legacy_path(references)?
                }
                Err(_) => return Err(configuration_missing()),
            }
        } else {
            Self::legacy_path(references)?
        };
        let config = read_config(path.clone()).await?;
        if references
            .iter()
            .any(|reference| reference.client_id != config.client_id())
        {
            return Err(error(
                "client_mismatch",
                "This profile uses a different Google app registration. Use its original configured build or desktop-client override.",
            ));
        }
        Ok((config, path))
    }

    /// Loads the registration an account was connected with, falling back to
    /// this build's file when the recorded path no longer exists. The client
    /// id must still match.
    pub(super) async fn load_saved(
        &self,
        reference: GoogleClientReference,
    ) -> Result<(OAuthClientConfig, String)> {
        let explicit = match tokio::fs::symlink_metadata(&reference.config_path).await {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            _ => Some(reference.config_path.clone()),
        };
        self.load(explicit, &[reference]).await
    }

    fn legacy_path(references: &[GoogleClientReference]) -> Result<String> {
        references
            .first()
            .map(|reference| reference.config_path.clone())
            .ok_or_else(configuration_missing)
    }
}

fn configuration_missing() -> CoreError {
    error(
        "google_not_configured",
        "Google sign-in is not configured in this build. Install a build with fuckMail's Google app registration, then try again.",
    )
}
