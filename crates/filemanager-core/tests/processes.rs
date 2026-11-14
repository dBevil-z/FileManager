use filemanager_core::process;
use std::{
    fs,
    time::{Duration, Instant},
};
#[test]
fn timeout_reaps_converter_group_including_pipe_holding_descendant() {
    let t = tempfile::tempdir().unwrap();
    let pidfile = t.path().join("child.pid");
    let start = Instant::now();
    let result = process::run(
        "sh",
        [
            "-c",
            "sleep 60 & echo $! > \"$1\"; wait",
            "probe",
            pidfile.to_str().unwrap(),
        ],
        None,
        1,
    );
    assert!(result.unwrap_err().contains("超时"));
    assert!(start.elapsed() < Duration::from_secs(5));
    let pid = fs::read_to_string(pidfile).unwrap();
    let status = fs::read_to_string(format!("/proc/{}/stat", pid.trim()));
    assert!(
        status.is_err()
            || status
                .unwrap()
                .rsplit(") ")
                .next()
                .unwrap()
                .starts_with('Z')
    );
}
