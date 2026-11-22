// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Cua AI, Inc.

//! The standalone connect stub must leave an existing agent registration alone.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::process::Command;

const SPACES_DRIVER: &str = "/Applications/Cua Spaces.app/Contents/Helpers/cua-driver";

#[test]
fn disabled_connect_preserves_an_agent_that_uses_the_driver_inside_cua_spaces() {
    let root = tempfile::tempdir().unwrap();
    let (home, bin) = (root.path().join("home"), root.path().join("bin"));
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&bin).unwrap();
    let claude = bin.join("claude");
    // `claude mcp add-json --scope user <name> <json>` writes the entry.
    std::fs::write(
        &claude,
        format!(
            "#!/bin/sh\n[ \"$2\" = add-json ] && printf '{{\"mcpServers\":{{\"%s\":%s}}}}' \"$5\" \"$6\" > '{}'\nexit 0\n",
            home.join(".claude.json").display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
    let config = serde_json::json!({ "mcpServers": { "cua-driver": {
        "command": SPACES_DRIVER,
        "args": ["mcp", "--embedded", "--socket", "/tmp/cua-driver.sock"],
    } } })
    .to_string();
    std::fs::write(home.join(".claude.json"), &config).unwrap();

    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_computer-use"))
            .args(args)
            .env("HOME", &home)
            .env("PATH", &bin)
            .env("CUA_DRIVER_RS_HOME", &home)
            .env("CUA_DRIVER_RS_TELEMETRY_ENABLED", "0")
            .env_remove("CLAUDE_CONFIG_DIR")
            .output()
            .unwrap()
    };

    for args in [
        &["connect", "--client", "claude", "--json"][..],
        &["connect", "--client", "claude", "--replace", "--json"][..],
    ] {
        let output = run(args);
        assert_eq!(output.status.code(), Some(2), "{output:?}");
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["ok"], false);
        let error = report["error"].as_str().expect("manual setup guidance");
        assert!(error.contains("connect is not available"), "{error}");
        assert!(error.contains("mcp"), "{error}");
        assert_eq!(
            std::fs::read_to_string(home.join(".claude.json")).unwrap(),
            config
        );
    }
}
