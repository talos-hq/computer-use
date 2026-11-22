// SPDX-License-Identifier: MIT
//! Busybox-style routing. Applet arguments never pass through the driver parser.
use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Applet {
    Driver,
    CursorTheme,
    Perception,
    #[cfg(windows)]
    Uia,
}

fn basename(argv0: &OsStr) -> String {
    let name = Path::new(argv0)
        .file_name()
        .unwrap_or(argv0)
        .to_string_lossy();
    name.strip_suffix(".exe").unwrap_or(&name).to_owned()
}

fn named(name: &str) -> Option<Applet> {
    match name {
        "computer-use" | "cua-driver" => Some(Applet::Driver),
        "cua-cursor-theme" | "computer-use-cursor-theme" => Some(Applet::CursorTheme),
        "cua-perception" | "computer-use-perception" => Some(Applet::Perception),
        #[cfg(windows)]
        "cua-driver-uia" => Some(Applet::Uia),
        _ => None,
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Invocation {
    Run(Applet, Vec<OsString>),
    List,
    InstallLinks(Vec<OsString>),
}

fn resolve(args: Vec<OsString>) -> anyhow::Result<Invocation> {
    let applet = args
        .first()
        .and_then(|arg| named(&basename(arg)))
        .unwrap_or(Applet::Driver);
    let mut args = args.into_iter().skip(1);
    if applet == Applet::Driver {
        let args: Vec<_> = args.by_ref().collect();
        if args.first().is_some_and(|arg| arg == "applet") {
            let name = args.get(1).ok_or_else(|| {
                anyhow::anyhow!("usage: computer-use applet <name> [args...] | --list")
            })?;
            if name == "--list" && args.len() == 2 {
                return Ok(Invocation::List);
            }
            let applet = match name.to_str() {
                Some("cursor-theme") => Some(Applet::CursorTheme),
                Some("perception") => Some(Applet::Perception),
                _ => named(&name.to_string_lossy()),
            }
            .filter(|applet| *applet != Applet::Driver)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "unknown applet {}; use computer-use applet --list",
                    name.to_string_lossy()
                )
            })?;
            return Ok(Invocation::Run(applet, args[2..].to_vec()));
        }
        if args.first().is_some_and(|arg| arg == "install-links") {
            return Ok(Invocation::InstallLinks(args[1..].to_vec()));
        }
        return Ok(Invocation::Run(applet, args));
    }
    Ok(Invocation::Run(applet, args.collect()))
}

pub(crate) fn driver_name() -> &'static str {
    if std::env::args_os()
        .next()
        .is_some_and(|arg| basename(&arg) == "cua-driver")
    {
        "cua-driver"
    } else {
        "computer-use"
    }
}

pub(crate) fn brand_help(text: &str) -> String {
    if driver_name() == "computer-use" {
        text.replace("cua-driver ", "computer-use ")
            .replace("cua-driver:", "computer-use:")
            .replace("Cua Driver", "computer-use")
    } else {
        text.to_owned()
    }
}

fn strings(args: Vec<OsString>) -> anyhow::Result<Vec<String>> {
    args.into_iter()
        .map(|arg| {
            arg.into_string()
                .map_err(|_| anyhow::anyhow!("applet arguments must be UTF-8"))
        })
        .collect()
}

/// None means the ordinary driver entry point should continue on the main thread.
pub(crate) fn dispatch(args: Vec<OsString>) -> anyhow::Result<Option<i32>> {
    let code = match resolve(args)? {
        Invocation::Run(Applet::Driver, _) => return Ok(None),
        Invocation::Run(Applet::CursorTheme, args) => cursor_theme_cli::run(strings(args)?),
        Invocation::Run(Applet::Perception, args) => cua_perception::cli::run(args),
        #[cfg(windows)]
        Invocation::Run(Applet::Uia, args) => {
            cua_driver_uia::run(strings(args)?)?;
            0
        }
        Invocation::List => {
            for name in LINK_NAMES.iter().filter(|name| **name != "cua-driver") {
                println!("{name}");
            }
            0
        }
        Invocation::InstallLinks(args) => {
            if args.as_slice() == [OsString::from("--help")] {
                println!("Usage: computer-use install-links [--dir DIR]");
                return Ok(Some(0));
            }
            let executable = std::env::current_exe()?;
            let directory = match args.as_slice() {
                [] => executable.parent().unwrap_or(Path::new(".")).to_path_buf(),
                [flag, directory] if flag == "--dir" => PathBuf::from(directory),
                _ => anyhow::bail!("usage: computer-use install-links [--dir DIR]"),
            };
            install_links(&executable, &directory)?;
            0
        }
    };
    Ok(Some(code))
}

const LINK_NAMES: &[&str] = &[
    "cua-driver",
    "cua-cursor-theme",
    "computer-use-cursor-theme",
    "cua-perception",
    "computer-use-perception",
    #[cfg(windows)]
    "cua-driver-uia",
];

fn install_links(executable: &Path, directory: &Path) -> anyhow::Result<()> {
    let executable = executable.canonicalize()?;
    std::fs::create_dir_all(directory)?;
    for name in LINK_NAMES {
        let name = if cfg!(windows) {
            format!("{name}.exe")
        } else {
            (*name).to_owned()
        };
        let link = directory.join(name);
        match std::fs::symlink_metadata(&link) {
            Ok(metadata)
                if metadata.file_type().is_symlink()
                    && link.canonicalize().ok().as_ref() == Some(&executable) =>
            {
                continue
            }
            Ok(_) => anyhow::bail!("refusing to overwrite {}", link.display()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(&executable, &link)?;
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&executable, &link)?;
        println!("{} -> {}", link.display(), executable.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn invocation(args: &[&str]) -> anyhow::Result<Invocation> {
        resolve(args.iter().map(OsString::from).collect())
    }
    #[test]
    fn argv0_basename_and_exe_suffix_select_applets() {
        for (name, applet) in [
            ("computer-use", Applet::Driver),
            ("cua-driver", Applet::Driver),
            ("cua-cursor-theme", Applet::CursorTheme),
            ("computer-use-cursor-theme", Applet::CursorTheme),
            ("cua-perception", Applet::Perception),
            ("computer-use-perception", Applet::Perception),
        ] {
            for path in [
                name.to_owned(),
                format!("/opt/bin/{name}"),
                format!("/opt/bin/{name}.exe"),
            ] {
                assert_eq!(
                    invocation(&[&path, "--help"]).unwrap(),
                    Invocation::Run(applet, vec!["--help".into()])
                );
            }
        }
        #[cfg(windows)]
        assert_eq!(
            invocation(&[r"C:\bin\cua-driver-uia.exe"]).unwrap(),
            Invocation::Run(Applet::Uia, vec![])
        );
    }
    #[test]
    fn unknown_argv0_falls_back_to_driver() {
        assert_eq!(
            invocation(&["/tmp/renamed", "call", "foo"]).unwrap(),
            Invocation::Run(Applet::Driver, vec!["call".into(), "foo".into()])
        );
        assert_eq!(
            invocation(&[]).unwrap(),
            Invocation::Run(Applet::Driver, vec![])
        );
        #[cfg(not(windows))]
        assert_eq!(
            invocation(&["cua-driver-uia"]).unwrap(),
            Invocation::Run(Applet::Driver, vec![])
        );
    }
    #[test]
    fn explicit_applet_preserves_arguments_and_bypasses_driver_flags() {
        assert_eq!(
            invocation(&["computer-use", "applet", "cua-perception", "--fixture"]).unwrap(),
            Invocation::Run(Applet::Perception, vec!["--fixture".into()])
        );
        assert_eq!(
            invocation(&["cua-driver", "applet", "cursor-theme", "--help"]).unwrap(),
            Invocation::Run(Applet::CursorTheme, vec!["--help".into()])
        );
        assert_eq!(
            invocation(&["computer-use", "applet", "--list"]).unwrap(),
            Invocation::List
        );
        for args in [
            vec!["computer-use", "applet"],
            vec!["computer-use", "applet", "unknown"],
            vec!["computer-use", "applet", "--list", "extra"],
        ] {
            assert!(invocation(&args).is_err());
        }
    }
    #[cfg(unix)]
    #[test]
    fn install_links_is_idempotent_and_preserves_existing_files() {
        let root = tempfile::tempdir().unwrap();
        let executable = root.path().join("computer-use");
        std::fs::write(&executable, b"binary").unwrap();
        let executable = executable.canonicalize().unwrap();
        let links = root.path().join("links");
        install_links(&executable, &links).unwrap();
        install_links(&executable, &links).unwrap();
        for name in LINK_NAMES {
            let name = if cfg!(windows) {
                format!("{name}.exe")
            } else {
                (*name).to_owned()
            };
            assert_eq!(links.join(name).canonicalize().unwrap(), executable);
        }
        let occupied = root.path().join("occupied");
        std::fs::create_dir(&occupied).unwrap();
        let file = occupied.join(if cfg!(windows) {
            "cua-driver.exe"
        } else {
            "cua-driver"
        });
        std::fs::write(&file, b"keep").unwrap();
        assert!(install_links(&executable, &occupied).is_err());
        assert_eq!(std::fs::read(file).unwrap(), b"keep");
    }
}
