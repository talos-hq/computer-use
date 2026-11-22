// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Cua AI, Inc.

//! Shared xdg-desktop-portal connection helpers.
//!
//! ashpd's convenience constructors cache their first session-bus connection
//! process-wide. That is unsafe for our portal callers because several of them
//! deliberately use short-lived Tokio runtimes: once the first runtime is
//! dropped, the cached zbus connection no longer has a running executor and a
//! later portal call can wait forever. Always give ashpd an explicit
//! connection whose lifetime is owned by the caller's runtime instead.

use std::path::{Path, PathBuf};
use std::time::Duration;

pub(crate) const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) async fn fresh_session_connection() -> anyhow::Result<zbus::Connection> {
    zbus::Connection::session()
        .await
        .map_err(|e| anyhow::anyhow!("xdg-desktop-portal session bus unreachable: {e}"))
}

/// One restore token for combined RemoteDesktop input and ScreenCast capture.
/// Old per-purpose tokens are intentionally ignored: upgrading may prompt once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(
    not(all(feature = "portal-input", feature = "portal-capture")),
    allow(dead_code)
)]
pub enum RestoreToken {
    /// Keyboard, pointer and monitor sources on the same RemoteDesktop session.
    Desktop,
}

#[cfg_attr(
    not(any(feature = "portal-input", feature = "portal-capture")),
    allow(dead_code)
)]
impl RestoreToken {
    fn file_name(self) -> &'static str {
        match self {
            Self::Desktop => "desktop-session.token",
        }
    }

    fn path(self) -> Option<PathBuf> {
        Some(restore_token_path_in(&dirs::config_dir()?, self))
    }

    pub fn read(self) -> Option<String> {
        read_restore_token_at(&self.path()?)
    }

    pub fn write(self, token: &str) -> anyhow::Result<()> {
        let path = self
            .path()
            .ok_or_else(|| anyhow::anyhow!("no config dir available for portal restore_token"))?;
        write_restore_token_at(&path, token)
    }
}

fn restore_token_path_in(config_dir: &Path, token: RestoreToken) -> PathBuf {
    config_dir.join("cua-driver").join(token.file_name())
}

fn read_restore_token_at(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn write_restore_token_at(path: &Path, token: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            anyhow::anyhow!(
                "failed to create {} for restore_token: {e}",
                parent.display()
            )
        })?;
    }
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| {
            anyhow::anyhow!(
                "failed to open portal restore_token at {}: {e}",
                path.display()
            )
        })?;
    file.write_all(token.as_bytes()).map_err(|e| {
        anyhow::anyhow!(
            "failed to write portal restore_token to {}: {e}",
            path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn unified_restore_token_has_one_file_under_the_driver_config_dir() {
        assert_eq!(
            restore_token_path_in(Path::new("/cfg"), RestoreToken::Desktop),
            PathBuf::from("/cfg/cua-driver/desktop-session.token")
        );
    }

    #[test]
    fn restore_token_roundtrips_trimmed_and_private() {
        let dir = tempfile::tempdir().unwrap();
        let path = restore_token_path_in(dir.path(), RestoreToken::Desktop);

        assert_eq!(read_restore_token_at(&path), None);
        write_restore_token_at(&path, "  tok-123 \n").unwrap();
        assert_eq!(read_restore_token_at(&path).as_deref(), Some("tok-123"));
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);

        write_restore_token_at(&path, "\n").unwrap();
        assert_eq!(read_restore_token_at(&path), None);
    }
}
