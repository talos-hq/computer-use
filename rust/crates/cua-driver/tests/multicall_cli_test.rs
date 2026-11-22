// SPDX-License-Identifier: MIT
use std::process::Command;

const BINARY: &str = env!("CARGO_BIN_EXE_computer-use");

#[test]
fn product_version_and_explicit_applets() {
    let output = Command::new(BINARY).arg("--version").output().unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("computer-use {}\n", env!("CARGO_PKG_VERSION"))
    );
    let list = Command::new(BINARY)
        .args(["applet", "--list"])
        .output()
        .unwrap();
    assert!(list.status.success());
    let list = String::from_utf8(list.stdout).unwrap();
    assert!(list.lines().any(|name| name == "cua-cursor-theme"));
    assert!(list.lines().any(|name| name == "cua-perception"));
    let explicit = Command::new(BINARY)
        .args(["applet", "cua-cursor-theme", "--help"])
        .output()
        .unwrap();
    let delegated = Command::new(BINARY)
        .args(["cursor-theme", "list", "--json"])
        .output()
        .unwrap();
    assert!(explicit.status.success());
    assert!(delegated.status.success());
    let listed = Command::new(BINARY)
        .args(["applet", "cua-cursor-theme", "list", "--json"])
        .output()
        .unwrap();
    assert!(listed.status.success());
    assert_eq!(listed.stdout, delegated.stdout);
    assert!(String::from_utf8(explicit.stdout)
        .unwrap()
        .contains("computer-use cursor-theme validate"));
    let unknown = Command::new(BINARY)
        .args(["applet", "missing", "--version"])
        .output()
        .unwrap();
    assert!(!unknown.status.success());
    assert!(unknown.stdout.is_empty());
}

#[cfg(unix)]
#[test]
fn installed_links_route_without_sibling_executables() {
    let root = tempfile::tempdir().unwrap();
    let output = Command::new(BINARY)
        .arg("install-links")
        .arg("--dir")
        .arg(root.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(root.path().join("cua-driver"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("cua-driver {}\n", env!("CARGO_PKG_VERSION"))
    );
    for name in ["cua-cursor-theme", "computer-use-cursor-theme"] {
        let output = Command::new(root.path().join(name))
            .arg("--help")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8(output.stdout)
            .unwrap()
            .contains("computer-use cursor-theme"));
    }
    for name in ["cua-perception", "computer-use-perception"] {
        let output = Command::new(root.path().join(name))
            .arg("--fixture")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
    for (name, expected) in [
        ("cua-driver.exe", "cua-driver"),
        ("unknown", "computer-use"),
    ] {
        let link = root.path().join(name);
        std::os::unix::fs::symlink(BINARY, &link).unwrap();
        let output = Command::new(link).arg("--version").output().unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!("{expected} {}\n", env!("CARGO_PKG_VERSION"))
        );
    }
}
