#![cfg(feature = "embed")]
use assert_cmd::Command;

#[test]
#[ignore] // network: first run downloads the model
fn embed_prints_384_dim_object() {
    let output = Command::cargo_bin("mem0")
        .unwrap()
        .args(["embed", "hello world"])
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let v: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("JSON on stdout");
    assert_eq!(v["dim"], 384);
    assert_eq!(v["model"], "multilingual-e5-small");
    assert_eq!(v["embedding"].as_array().unwrap().len(), 384);
}
