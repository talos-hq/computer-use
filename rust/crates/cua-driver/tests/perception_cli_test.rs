// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Cua AI, Inc.

use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::process::{Command, Output};

use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use serde_json::json;
use tempfile::TempDir;

fn run(home: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_computer-use"))
        .args(args)
        .env("CUA_DRIVER_RS_HOME", home)
        .env_remove("CUA_PERCEPTION_MANIFEST")
        .env_remove("CUA_PERCEPTION_RUNTIME")
        .env("CUA_DRIVER_CLI_TELEMETRY_CHILD", "1")
        .output()
        .expect("run cua-driver perception command")
}

fn write_fixture(temp: &TempDir) -> (String, String) {
    let image_path = temp.path().join("input.png");
    let mut bytes = Vec::new();
    DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 1, Rgba([1, 2, 3, 255])))
        .write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Png)
        .unwrap();
    fs::write(&image_path, bytes).unwrap();

    let capture_path = temp.path().join("capture.json");
    fs::write(
        &capture_path,
        serde_json::to_vec(&json!({
            "source": {"kind": "window", "pid": 123, "window_id": 456},
            "snapshot_id": "snapshot-local-fixture"
        }))
        .unwrap(),
    )
    .unwrap();
    (
        image_path.to_string_lossy().into_owned(),
        capture_path.to_string_lossy().into_owned(),
    )
}

fn parse_error(output: &Output) -> serde_json::Value {
    assert!(!output.status.success());
    assert!(
        output.stderr.is_empty(),
        "stderr was not empty: {:?}",
        output.stderr
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["ok"], false);
    value
}

fn run_parse(home: &std::path::Path, image: &str, capture: &str) -> Output {
    run(
        home,
        &[
            "perception",
            "parse",
            "--image",
            image,
            "--capture",
            capture,
            "--json",
        ],
    )
}

#[test]
fn installed_extension_executable_is_not_selected() {
    let temp = TempDir::new().unwrap();
    let extension = temp.path().join("extensions/cua-perception");
    fs::create_dir_all(&extension).unwrap();
    fs::write(extension.join("active.json"), br#"{"version":"1.0.0"}"#).unwrap();
    let (image, capture) = write_fixture(&temp);
    let output = run_parse(temp.path(), &image, &capture);
    let error = parse_error(&output);
    assert_eq!(error["error"]["code"], "artifact_invalid");
    assert!(String::from_utf8_lossy(&output.stdout).contains("missing perception model manifest"));
}

#[test]
fn help_documents_local_parse_as_non_actionable() {
    let temp = TempDir::new().unwrap();
    let output = run(temp.path(), &["--help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("perception parse --image <png> --capture <capture.json> --json"));
    assert!(stdout.contains("no Driver action authority"));

    let manifest = run(temp.path(), &["manifest"]);
    assert!(manifest.status.success());
    let manifest_json: serde_json::Value = serde_json::from_slice(&manifest.stdout).unwrap();
    let perception = manifest_json["subcommands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|command| command["name"] == "perception")
        .expect("manifest perception command");
    assert!(perception["description"]
        .as_str()
        .unwrap()
        .contains("without creating Driver capture or action authority"));

    let docs = run(temp.path(), &["dump-docs", "--type", "commands"]);
    assert!(docs.status.success());
    let docs_text = String::from_utf8_lossy(&docs.stdout);
    assert!(docs_text.contains("local-input mode does not create a Driver capture"));
}

#[test]
fn valid_local_input_without_assets_reports_missing_manifest() {
    let temp = TempDir::new().unwrap();
    let (image, capture) = write_fixture(&temp);
    let output = run_parse(temp.path(), &image, &capture);
    assert_eq!(parse_error(&output)["error"]["code"], "artifact_invalid");
    assert!(!temp.path().join("extensions").exists());
}

#[test]
fn invalid_metadata_and_png_fail_before_worker_launch() {
    let temp = TempDir::new().unwrap();
    let (image, capture) = write_fixture(&temp);
    fs::write(
        &capture,
        br#"{"source":{"kind":"window","pid":0,"window_id":1}}"#,
    )
    .unwrap();
    let invalid_metadata = run_parse(temp.path(), &image, &capture);
    assert_eq!(
        parse_error(&invalid_metadata)["error"]["code"],
        "invalid_capture_metadata"
    );

    let (_, capture) = write_fixture(&temp);
    fs::write(&image, b"not a png").unwrap();
    let invalid_png = run_parse(temp.path(), &image, &capture);
    assert_eq!(parse_error(&invalid_png)["error"]["code"], "invalid_png");
}

#[test]
fn invalid_installed_extension_fails_as_artifact_invalid() {
    let temp = TempDir::new().unwrap();
    let (image, capture) = write_fixture(&temp);
    let extension = temp.path().join("extensions/cua-perception");
    fs::create_dir_all(&extension).unwrap();
    fs::write(extension.join("active.json"), b"not json").unwrap();
    let output = run_parse(temp.path(), &image, &capture);
    assert_eq!(parse_error(&output)["error"]["code"], "artifact_invalid");
}

#[test]
fn argument_failure_has_an_exact_json_envelope() {
    let temp = TempDir::new().unwrap();
    let output = run(temp.path(), &["perception", "parse", "--json"]);
    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "{\"ok\":false,\"error\":{\"code\":\"invalid_arguments\",\"message\":\"perception parse requires --image <png>\",\"retryable\":false}}\n"
    );
}

#[test]
fn rejects_unknown_nested_source_fields_and_malformed_timestamp() {
    let temp = TempDir::new().unwrap();
    let (image, capture) = write_fixture(&temp);
    fs::write(
        &capture,
        br#"{"source":{"kind":"window","pid":123,"window_id":456,"extra":true}}"#,
    )
    .unwrap();
    let unknown = run_parse(temp.path(), &image, &capture);
    assert_eq!(
        parse_error(&unknown)["error"]["code"],
        "invalid_capture_metadata"
    );

    fs::write(
        &capture,
        br#"{"source":{"kind":"window","pid":123,"window_id":456},"captured_at":"yesterday"}"#,
    )
    .unwrap();
    let timestamp = run_parse(temp.path(), &image, &capture);
    let error = parse_error(&timestamp);
    assert_eq!(error["error"]["code"], "invalid_capture_metadata");
    assert_eq!(
        error["error"]["message"],
        "captured_at must be an RFC3339 timestamp"
    );
}

#[cfg(unix)]
#[test]
fn rejects_symlink_and_fifo_inputs_without_blocking() {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::symlink;

    let temp = TempDir::new().unwrap();
    let (image, capture) = write_fixture(&temp);
    let image_link = temp.path().join("image-link.png");
    symlink(&image, &image_link).unwrap();
    let symlink_output = run_parse(temp.path(), image_link.to_str().unwrap(), capture.as_str());
    assert_eq!(
        parse_error(&symlink_output)["error"]["code"],
        "input_open_failed"
    );

    let fifo = temp.path().join("capture.fifo");
    let fifo_c = CString::new(fifo.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) }, 0);
    let fifo_output = run_parse(temp.path(), &image, fifo.to_str().unwrap());
    assert_eq!(
        parse_error(&fifo_output)["error"]["code"],
        "input_not_regular_file"
    );
}
