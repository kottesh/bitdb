use std::process::Command;

use tempfile::tempdir;

#[test]
fn cli_help_succeeds() {
    let bin = env!("CARGO_BIN_EXE_bitdb");
    let output = Command::new(bin)
        .arg("--help")
        .output()
        .expect("binary should run");

    assert!(output.status.success());
}

#[test]
fn cli_put_get_delete_flow() {
    let dir = tempdir().expect("tempdir should be created");
    let bin = env!("CARGO_BIN_EXE_bitdb");

    let put = Command::new(bin)
        .args([
            "--data-dir",
            dir.path().to_str().expect("utf8 path"),
            "put",
            "alpha",
            "1",
        ])
        .output()
        .expect("put should run");
    assert!(put.status.success());

    let get = Command::new(bin)
        .args([
            "--data-dir",
            dir.path().to_str().expect("utf8 path"),
            "get",
            "alpha",
        ])
        .output()
        .expect("get should run");
    assert!(get.status.success());
    assert_eq!(String::from_utf8_lossy(&get.stdout).trim(), "1");

    let delete = Command::new(bin)
        .args([
            "--data-dir",
            dir.path().to_str().expect("utf8 path"),
            "delete",
            "alpha",
        ])
        .output()
        .expect("delete should run");
    assert!(delete.status.success());

    let get_missing = Command::new(bin)
        .args([
            "--data-dir",
            dir.path().to_str().expect("utf8 path"),
            "get",
            "alpha",
        ])
        .output()
        .expect("get should run");
    assert!(get_missing.status.success());
    assert_eq!(
        String::from_utf8_lossy(&get_missing.stdout).trim(),
        "NOT_FOUND"
    );
}

#[test]
fn cli_stats_and_merge_commands_work() {
    let dir = tempdir().expect("tempdir should be created");
    let bin = env!("CARGO_BIN_EXE_bitdb");

    let _ = Command::new(bin)
        .args([
            "--data-dir",
            dir.path().to_str().expect("utf8 path"),
            "put",
            "k",
            "v",
        ])
        .output()
        .expect("put should run");

    let stats = Command::new(bin)
        .args([
            "--data-dir",
            dir.path().to_str().expect("utf8 path"),
            "stats",
        ])
        .output()
        .expect("stats should run");
    assert!(stats.status.success());
    let stats_out = String::from_utf8_lossy(&stats.stdout);
    assert!(stats_out.contains("live_keys="));
    assert!(stats_out.contains("tombstones="));

    let merge = Command::new(bin)
        .args([
            "--data-dir",
            dir.path().to_str().expect("utf8 path"),
            "merge",
        ])
        .output()
        .expect("merge should run");
    assert!(merge.status.success());
}
