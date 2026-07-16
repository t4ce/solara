use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::Value;

const BINARY: &str = env!("CARGO_BIN_EXE_rust-qjs-dom");

#[test]
fn file_mode_emits_a_valid_artifact() {
    let output = Command::new(BINARY)
        .args([
            "--compact",
            "--url",
            "https://example.test/forms",
            "tests/fixtures/forms.html",
        ])
        .output()
        .expect("CLI starts");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let artifact: Value = serde_json::from_slice(&output.stdout).expect("valid output JSON");
    assert_eq!(artifact["schema"], "rustqjsdom.artifact");
    assert_eq!(artifact["source"]["url"], "https://example.test/forms");
}

#[test]
fn jsonl_mode_reports_bad_input_and_continues() {
    let mut child = Command::new(BINARY)
        .arg("--jsonl")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("CLI starts");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"{bad json}\n{\"url\":\"proof:\",\"html\":\"<p>alive</p>\"}\n")
        .expect("requests write");
    let output = child.wait_with_output().expect("CLI completes");
    assert!(output.status.success());

    let records = String::from_utf8(output.stdout).expect("UTF-8 output");
    let records = records
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("valid record"))
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["schema"], "rustqjsdom.error");
    assert_eq!(records[1]["schema"], "rustqjsdom.artifact");
    assert_eq!(records[1]["source"]["url"], "proof:");
}
