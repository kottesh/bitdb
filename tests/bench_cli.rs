use std::process::Command;

use tempfile::tempdir;

#[test]
fn cli_bench_startup_serial_runs() {
    let dir = tempdir().expect("tempdir should be created");
    let bin = env!("CARGO_BIN_EXE_bitdb");

    let out = Command::new(bin)
        .args([
            "--data-dir",
            dir.path().to_str().expect("utf8 path"),
            "bench",
            "startup",
            "--mode",
            "serial",
        ])
        .output()
        .expect("bench startup should run");

    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("startup_ms="));
}

#[test]
fn cli_bench_merge_serial_runs() {
    let dir = tempdir().expect("tempdir should be created");
    let bin = env!("CARGO_BIN_EXE_bitdb");

    let out = Command::new(bin)
        .args([
            "--data-dir",
            dir.path().to_str().expect("utf8 path"),
            "bench",
            "merge",
            "--mode",
            "serial",
        ])
        .output()
        .expect("bench merge should run");

    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("merge_ms="));
}

#[test]
fn cli_bench_workload_runs() {
    let dir = tempdir().expect("tempdir should be created");
    let bin = env!("CARGO_BIN_EXE_bitdb");

    let out = Command::new(bin)
        .args([
            "--data-dir",
            dir.path().to_str().expect("utf8 path"),
            "bench",
            "workload",
            "--ops",
            "50",
            "--mode",
            "serial",
            "--threads",
            "1",
        ])
        .output()
        .expect("bench workload should run");

    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("ops_per_sec="));
}
