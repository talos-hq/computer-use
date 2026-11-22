// SPDX-License-Identifier: MIT
//! The worker code is built in; inference assets are supplied locally.
use anyhow::{Context, Result};
use cua_driver_core::perception_client::{
    PerceptionClient, PerceptionClientResolver, PerceptionWorkerConfig, ResolutionDeferred,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

fn asset_paths() -> Result<(PathBuf, PathBuf)> {
    let home = crate::env_vars::var_os("CUA_DRIVER_RS_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let home = match home {
        Some(home) => home,
        None => {
            #[cfg(windows)]
            let home = std::env::var_os("USERPROFILE").context("USERPROFILE not set")?;
            #[cfg(not(windows))]
            let home = std::env::var_os("HOME").context("HOME not set")?;
            PathBuf::from(home).join(crate::bundle::user_home_subdirectory())
        }
    };
    let root = home.join("perception");
    let manifest = crate::env_vars::var_os("CUA_PERCEPTION_MANIFEST")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("model-manifest.json"));
    let library = if cfg!(windows) {
        "onnxruntime.dll"
    } else if cfg!(target_os = "macos") {
        "libonnxruntime.dylib"
    } else {
        "libonnxruntime.so"
    };
    let runtime = crate::env_vars::var_os("CUA_PERCEPTION_RUNTIME")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("runtime").join(library));
    Ok((manifest, runtime))
}

fn worker_config(
    executable: PathBuf,
    manifest: &Path,
    runtime: &Path,
) -> Result<PerceptionWorkerConfig> {
    // Canonical paths survive the worker's private working directory. The
    // existing worker verifies model/runtime hashes and runtime version.
    let manifest = manifest.canonicalize().with_context(|| {
        format!(
            "missing perception model manifest {}; supply local assets as described in README",
            manifest.display()
        )
    })?;
    let runtime = runtime.canonicalize().with_context(|| format!("missing perception ONNX Runtime library {}; supply local assets as described in README", runtime.display()))?;
    let mut config = PerceptionWorkerConfig::installed_with_identity(
        executable,
        "cua-perception",
        env!("CARGO_PKG_VERSION"),
    );
    config = config.with_executable_file_only();
    let identity = config.args;
    config.args = vec![
        "applet".into(),
        "cua-perception".into(),
        "--manifest".into(),
        manifest
            .to_str()
            .context("perception manifest path must be UTF-8")?
            .into(),
        "--onnx-runtime-library".into(),
        runtime
            .to_str()
            .context("perception runtime path must be UTF-8")?
            .into(),
    ];
    config.args.extend(identity);
    Ok(config)
}

pub(crate) fn client() -> Result<PerceptionClient> {
    let (manifest, runtime) = asset_paths()?;
    let config = worker_config(std::env::current_exe()?, &manifest, &runtime)?;
    PerceptionClient::new(config).map_err(|error| anyhow::anyhow!("{}", error.message))
}

pub(crate) fn resolver() -> Arc<dyn PerceptionClientResolver> {
    Arc::new(BuiltinResolver)
}
struct BuiltinResolver;
impl PerceptionClientResolver for BuiltinResolver {
    fn fingerprint(&self) -> String {
        match asset_paths() {
            Ok((manifest, runtime)) => [manifest, runtime]
                .iter()
                .map(|path| {
                    let metadata = std::fs::metadata(path).map(|m| (m.len(), m.modified().ok()));
                    format!("{}:{metadata:?}", path.display())
                })
                .collect::<Vec<_>>()
                .join("|"),
            Err(error) => format!("unavailable:{error:#}"),
        }
    }
    fn resolve(&self) -> Result<PerceptionClient, ResolutionDeferred> {
        Ok(client().unwrap_or_else(|error| {
            tracing::warn!("built-in perception assets unavailable: {error:#}");
            PerceptionClient::unavailable()
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn worker_is_current_binary_applet_with_absolute_local_assets() {
        let root = tempfile::tempdir().unwrap();
        let manifest = root.path().join("model-manifest.json");
        let runtime = root.path().join("libonnxruntime.so");
        std::fs::write(&manifest, b"{}").unwrap();
        std::fs::write(&runtime, b"library").unwrap();
        let executable = std::env::current_exe().unwrap();
        let config = worker_config(executable.clone(), &manifest, &runtime).unwrap();
        assert_eq!(config.executable, executable);
        assert_eq!(&config.args[..2], ["applet", "cua-perception"]);
        assert_eq!(Path::new(&config.args[3]), manifest.canonicalize().unwrap());
        assert_eq!(Path::new(&config.args[5]), runtime.canonicalize().unwrap());
        assert_eq!(
            config.expected_extension_identity.unwrap().id,
            "cua-perception"
        );
        assert!(config.warm_worker.is_none());
    }
    #[test]
    fn missing_assets_are_reported_without_installing_anything() {
        let root = tempfile::tempdir().unwrap();
        let error = worker_config(
            std::env::current_exe().unwrap(),
            &root.path().join("missing.json"),
            &root.path().join("missing.so"),
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("missing perception model manifest"));
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }
}
