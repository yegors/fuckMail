//! App-owned OAuth registration. User grants remain exclusively in the vault.
use super::*;
use std::path::PathBuf;

#[cfg(not(feature = "verification-google"))]
pub(super) const APP_CLIENT_FILE: &str = "google-oauth-client.json";

#[derive(Default)]
pub(super) struct AppClient {
    // Captured once at engine startup; tests and isolated fixtures inject a path.
    path: Option<PathBuf>,
    required: bool,
}

impl AppClient {
    #[cfg(not(feature = "verification-google"))]
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

    pub(super) fn configured(path: PathBuf) -> Self {
        Self {
            path: Some(path),
            required: true,
        }
    }

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
