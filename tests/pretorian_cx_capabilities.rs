use std::process::Command;

#[test]
fn reports_cx_capabilities_without_starting_the_interface() {
    let output = Command::new(env!("CARGO_BIN_EXE_herdr"))
        .arg("--pretorian-cx-capabilities")
        .env("PRETORIAN_CX", "1")
        .output()
        .expect("terminal executável");
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON válido");
    assert_eq!(json, serde_json::json!({"version": 1}));
    assert!(output.stderr.is_empty());
}
