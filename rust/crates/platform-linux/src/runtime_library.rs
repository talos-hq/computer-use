//! Shared search for optional desktop libraries. No dependency on a Nix store path
//! from the build machine: libraries belong to the running desktop session.
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

fn candidates(soname: &str, paths: &[Option<std::ffi::OsString>], maps: &str) -> Vec<PathBuf> {
    let mut out = vec![PathBuf::from(soname)]; // ld.so cache, RUNPATH, LD_LIBRARY_PATH
    let mut add = |path: PathBuf| {
        if !out.contains(&path) {
            out.push(path);
        }
    };
    for value in paths.iter().flatten() {
        for dir in std::env::split_paths(value).filter(|p| p.is_absolute()) {
            add(dir.join(soname));
        }
    }
    for dir in [
        "/run/current-system/sw/lib",
        "/run/current-system/sw/share/nix-ld/lib",
        "/usr/lib/x86_64-linux-gnu",
        "/lib/x86_64-linux-gnu",
        "/usr/lib/aarch64-linux-gnu",
        "/lib/aarch64-linux-gnu",
        "/usr/lib64",
        "/lib64",
        "/usr/lib",
        "/lib",
    ] {
        add(Path::new(dir).join(soname));
    }
    // /proc maps are untrusted text. Only use absolute, live file names with
    // the requested library's basename (possibly a versioned real filename).
    for line in maps.lines() {
        if line.ends_with(" (deleted)") {
            continue;
        }
        let Some(start) = line.find('/') else {
            continue;
        };
        let path = Path::new(&line[start..]);
        if path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|name| name == soname || name.starts_with(&format!("{soname}.")))
        {
            add(path.to_owned());
        }
    }
    out
}

fn session_maps() -> String {
    use std::os::unix::fs::MetadataExt;
    let mut maps = std::fs::read_to_string("/proc/self/maps").unwrap_or_default();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return maps;
    };
    // SAFETY: geteuid has no preconditions.
    let uid = unsafe { libc::geteuid() };
    for entry in entries.flatten() {
        if entry
            .file_name()
            .to_str()
            .is_none_or(|s| !s.bytes().all(|b| b.is_ascii_digit()))
            || !entry.metadata().is_ok_and(|m| m.uid() == uid)
        {
            continue;
        }
        let base = entry.path();
        let comm = std::fs::read_to_string(base.join("comm")).unwrap_or_default();
        if [
            "kwin_wayland",
            "kwin_x11",
            "plasmashell",
            "gnome-shell",
            "sway",
            "niri",
            "Hyprland",
            "Xwayland",
            "pipewire",
            "wireplumber",
            "xdg-desktop-por", // Linux comm truncates portal executable names.
        ]
        .contains(&comm.trim())
        {
            maps.push_str(&std::fs::read_to_string(base.join("maps")).unwrap_or_default());
        }
    }
    maps
}

/// # Safety
/// The caller must establish the library's load-time initializer requirements.
pub(crate) unsafe fn open(soname: &str) -> Result<libloading::Library> {
    let paths = [
        "COMPUTER_USE_LIBRARY_PATH",
        "LD_LIBRARY_PATH",
        "NIX_LD_LIBRARY_PATH",
    ]
    .map(std::env::var_os);
    // Avoid reading other processes on conventional systems when the loader works.
    let initial = candidates(soname, &paths, "");
    let mut errors = Vec::new();
    // SAFETY: the caller establishes load-time requirements for both passes.
    if let Some(library) = unsafe { try_candidates(initial.clone(), &mut errors) } {
        return Ok(library);
    }
    let fallback = candidates(soname, &[], &session_maps())
        .into_iter()
        .filter(|path| !initial.contains(path));
    if let Some(library) = unsafe { try_candidates(fallback, &mut errors) } {
        return Ok(library);
    }
    Err(anyhow!("{soname} unavailable; install the desktop library or set COMPUTER_USE_LIBRARY_PATH to absolute library directories. Tried: {}", errors.join("; ")))
}

/// # Safety
/// Same load-time requirements as `open`.
unsafe fn try_candidates(
    paths: impl IntoIterator<Item = PathBuf>,
    errors: &mut Vec<String>,
) -> Option<libloading::Library> {
    for path in paths {
        // SAFETY: caller establishes load-time requirements.
        match unsafe { libloading::Library::new(&path) } {
            Ok(library) => return Some(library),
            Err(error) => errors.push(format!("{}: {error}", path.display())),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn environment_system_and_session_candidates_are_ordered_and_deduplicated() {
        let paths = [Some("/custom:/custom:relative:/nix-ld".into())];
        let list = candidates("libX11.so.6", &paths,
            "7f r-xp 0 00 0 /nix/store/session/lib/libX11.so.6.4.0\n7f r-xp 0 00 0 /nix/store/wrong/libXi.so.6\n7f r-xp 0 00 0 /tmp/libX11.so.6.4.0 (deleted)\n");
        assert_eq!(
            &list[..3],
            &[
                PathBuf::from("libX11.so.6"),
                "/custom/libX11.so.6".into(),
                "/nix-ld/libX11.so.6".into()
            ]
        );
        assert!(list.contains(&"/run/current-system/sw/lib/libX11.so.6".into()));
        assert_eq!(
            list.last().unwrap(),
            &PathBuf::from("/nix/store/session/lib/libX11.so.6.4.0")
        );
        assert!(!list.iter().any(|p| p.starts_with("relative")));
    }
    #[test]
    fn loader_falls_back_to_an_absolute_candidate() {
        let maps = std::fs::read_to_string("/proc/self/maps").unwrap();
        let libc = candidates("libc.so.6", &[], &maps)
            .into_iter()
            .find(|path| path.is_absolute() && path.exists())
            .unwrap();
        let mut errors = Vec::new();
        let library =
            unsafe { try_candidates(["/nonexistent/libc.so.6".into(), libc], &mut errors) }
                .expect("glibc fallback");
        assert_eq!(errors.len(), 1);
        // SAFETY: getpid's ABI is fixed and library remains alive.
        let getpid =
            unsafe { library.get::<unsafe extern "C" fn() -> libc::pid_t>(b"getpid\0") }.unwrap();
        assert_eq!(unsafe { getpid() }, std::process::id() as libc::pid_t);
    }
    #[test]
    fn no_desktop_libraries_are_needed_by_the_elf_loader() {
        let exe = std::env::current_exe().unwrap();
        let output = std::process::Command::new("readelf")
            .arg("-d")
            .arg(exe)
            .output()
            .unwrap();
        assert!(output.status.success());
        let dynamic = String::from_utf8_lossy(&output.stdout);
        for soname in [
            "libX11.so",
            "libXi.so",
            "libXtst.so",
            "libXext.so",
            "libxkbcommon.so",
            "libpipewire",
        ] {
            assert!(
                !dynamic
                    .lines()
                    .any(|line| line.contains("NEEDED") && line.contains(soname)),
                "unexpected link to {soname}: {dynamic}"
            );
        }
    }
    #[test]
    fn missing_library_returns_actionable_error() {
        let error = unsafe { open("libcomputer_use_nonexistent.so.0") }
            .unwrap_err()
            .to_string();
        assert!(error.contains("COMPUTER_USE_LIBRARY_PATH"));
        assert!(error.contains("/run/current-system/sw/lib"));
    }
}
