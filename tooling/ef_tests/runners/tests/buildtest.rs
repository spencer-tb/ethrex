//! `buildtest` builds each payload a fixture expects to be valid through
//! `testing_buildBlockV1`, compares it with the fixture's and then imports the
//! fixture's payload, and skips the payloads a fixture expects to be invalid.
//!
//! `buildtest_blob.json` is an EELS fixture whose Cancun payload carries a blob
//! transaction as payloads do, without its blobs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Map, Value, json};

const BLOB: &str = "tests/cancun/eip4844_blobs/test_blob_txs.py::test_blob_tx_attribute_value_opcode[fork_Cancun-blockchain_test_engine_from_state_test-tx_gas_500000-tx_value_0-opcode_CALLVALUE]";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// A fresh directory for `test`.
fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(test);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(args: &[&str], paths: &[&Path]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_buildtest"))
        .arg("--json")
        .args(args)
        .args(paths)
        .output()
        .expect("failed to run the binary")
}

/// The results in the one JSON array on stdout, by fixture name.
fn results(output: &Output) -> HashMap<String, Value> {
    let results: Vec<Value> =
        serde_json::from_slice(&output.stdout).expect("stdout is one JSON array");
    results
        .into_iter()
        .map(|r| (r["name"].as_str().unwrap().to_string(), r))
        .collect()
}

#[test]
fn valid_payloads_are_built_as_the_fixtures_have_them() {
    // A Paris payload, an Amsterdam one with its access list, and a Cancun blob
    // transaction without its blobs.
    let output = run(
        &[],
        &[
            &fixture("enginetest_rejections.json"),
            &fixture("enginetest_bal_report.json"),
            &fixture("buildtest_blob.json"),
        ],
    );
    let results = results(&output);
    for name in ["clean", "listed", BLOB] {
        let result = &results[name];
        assert_eq!(result["pass"], true, "{name}: {result}");
        assert_eq!(result["built"], 1, "{name}: {result}");
        assert_eq!(result["mismatches"], json!([]), "{name}: {result}");
        assert_eq!(result["rejections"], json!([]), "{name}: {result}");
    }
}

#[test]
fn invalid_payloads_are_neither_built_nor_imported() {
    let output = run(
        &[],
        &[
            &fixture("enginetest_rejections.json"),
            &fixture("enginetest_mismatched_list.json"),
        ],
    );
    let results = results(&output);
    for name in [
        "rejected_twice",
        "rejected_for_another_reason",
        "json_rpc_error",
        "mismatched_list",
    ] {
        let result = &results[name];
        assert_eq!(result["pass"], true, "{name}: {result}");
        assert_eq!(result["built"], 0, "{name}: {result}");
        assert_eq!(result["rejections"], json!([]), "{name}: {result}");
    }
}

#[test]
fn a_built_payload_that_differs_is_a_mismatch() {
    // ethrex builds this extra data into the block, which then hashes to another
    // block than the fixture names.
    let mut fixtures: Map<String, Value> =
        serde_json::from_slice(&std::fs::read(fixture("enginetest_bal_report.json")).unwrap())
            .unwrap();
    let payload = &mut fixtures["listed"]["engineNewPayloads"][0]["params"][0];
    let block_hash = payload["blockHash"].clone();
    payload["extraData"] = "0x01".into();
    let path = scratch("buildtest_mismatch").join("enginetest_bal_report.json");
    std::fs::write(&path, serde_json::to_vec(&fixtures).unwrap()).unwrap();

    let output = run(&[], &[&path]);

    assert_eq!(output.status.code(), Some(1));
    let result = &results(&output)["listed"];
    assert_eq!(result["pass"], false, "{result}");
    assert_eq!(result["built"], 1, "{result}");
    let mismatches = result["mismatches"].as_array().unwrap();
    assert_eq!(mismatches.len(), 1, "{mismatches:?}");
    assert_eq!(mismatches[0]["index"], 0);
    assert_eq!(mismatches[0]["field"], "blockHash");
    assert_eq!(mismatches[0]["expected"], block_hash);
    assert_ne!(mismatches[0]["got"], block_hash);
}

#[test]
fn buildtest_takes_no_bal_flags() {
    for flag in ["--bal-sequential", "--bal-report"] {
        let output = run(&[flag], &[&fixture("enginetest_bal_report.json")]);
        assert_eq!(output.status.code(), Some(2), "{flag}");
    }
}
