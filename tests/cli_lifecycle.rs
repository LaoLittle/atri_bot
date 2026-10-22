//! End-to-end CLI behavior against isolated deployment directories.
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    time::{Duration, Instant},
};

fn invoke(directory: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_atri"))
        .arg("--bot-dir")
        .arg(directory)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn offline_status_is_real_and_does_not_start_a_runtime() {
    let directory = tempfile::tempdir().unwrap();
    let status = invoke(directory.path(), &["status"]);
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    let stop = invoke(directory.path(), &["stop"]);
    assert_eq!(stop.status.code(), Some(1));
}

#[test]
fn generic_event_timeout_exits_124_and_releases_deployment_lock() {
    let directory = tempfile::tempdir().unwrap();
    let run = invoke(
        directory.path(),
        &[
            "run",
            "--headless",
            "--until-event",
            "example.never-published",
            "--timeout",
            "1",
        ],
    );
    assert_eq!(
        run.status.code(),
        Some(124),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(String::from_utf8_lossy(&run.stderr).contains("example.never-published"));
    let status = invoke(directory.path(), &["status"]);
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    assert!(String::from_utf8_lossy(&status.stdout).contains("offline"));
}

struct Running(Child);
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn deploy_binary(directory: &Path) -> PathBuf {
    let source = Path::new(env!("CARGO_BIN_EXE_atri"));
    let destination = directory.join(source.file_name().unwrap());
    std::fs::copy(source, &destination).unwrap();
    destination
}

fn deployed_command(binary: &Path, cwd: &Path, override_dir: Option<&Path>) -> Command {
    let mut command = Command::new(binary);
    command.current_dir(cwd);
    if let Some(directory) = override_dir {
        command.arg("--bot-dir").arg(directory);
    }
    command
}

async fn assert_control_lifecycle(
    binary: &Path,
    cwd: &Path,
    override_dir: Option<&Path>,
    deployment: &Path,
    args: &[&str],
) {
    let child = deployed_command(binary, cwd, override_dir)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut running = Running(child);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            running.0.try_wait().unwrap().is_none(),
            "runtime exited before control became available"
        );
        if let Some(client) = atri_bot::control::ControlClient::connect(deployment)
            .await
            .unwrap()
        {
            let value = client
                .request(atri_core::Request::new("runtime.status", ()).unwrap())
                .await
                .unwrap();
            let snapshot: atri_core::RuntimeSnapshot = atri_core::from_value(&value).unwrap();
            if snapshot.running {
                break;
            }
        }
        assert!(
            Instant::now() < deadline,
            "control endpoint did not become available"
        );
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    let status = deployed_command(binary, cwd, override_dir)
        .arg("status")
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    let stopped = deployed_command(binary, cwd, override_dir)
        .arg("stop")
        .output()
        .unwrap();
    assert!(
        stopped.status.success(),
        "{}",
        String::from_utf8_lossy(&stopped.stderr)
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = running.0.try_wait().unwrap() {
            assert!(status.success(), "runtime exited with {status}");
            break;
        }
        assert!(
            Instant::now() < deadline,
            "runtime did not shut down after stop"
        );
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    assert!(
        atri_bot::control::ControlClient::connect(deployment)
            .await
            .unwrap()
            .is_none(),
        "control endpoint remained active after orderly shutdown"
    );
    let _writer = atri_bot::storage::Storage::open(deployment.to_owned()).unwrap();
}

#[tokio::test]
async fn live_control_status_and_stop_operate_on_the_running_process() {
    let directory = tempfile::tempdir().unwrap();
    assert_control_lifecycle(
        Path::new(env!("CARGO_BIN_EXE_atri")),
        directory.path(),
        Some(directory.path()),
        directory.path(),
        &["run", "--headless", "--timeout", "15"],
    )
    .await;
}

#[tokio::test]
async fn no_argument_startup_uses_executable_deployment_from_inside_and_outside() {
    let deployment = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let binary = deploy_binary(deployment.path());
    for cwd in [outside.path(), deployment.path()] {
        assert_control_lifecycle(&binary, cwd, None, deployment.path(), &[]).await;
    }
    assert!(!outside.path().join("botDir").exists());
    assert!(!deployment.path().join("botDir").exists());
}

#[tokio::test]
async fn default_startup_and_management_preserve_explicit_deployment_override() {
    let executable_directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let deployment = tempfile::tempdir().unwrap();
    let binary = deploy_binary(executable_directory.path());
    assert_control_lifecycle(
        &binary,
        outside.path(),
        Some(deployment.path()),
        deployment.path(),
        &[],
    )
    .await;
    assert!(!executable_directory.path().join("extensions").exists());
    assert!(!outside.path().join("botDir").exists());
}

#[test]
fn service_call_requires_a_live_runtime_and_valid_json() {
    let directory = tempfile::tempdir().unwrap();
    let invalid = invoke(
        directory.path(),
        &[
            "services",
            "call",
            "example.challenge",
            "submit",
            "--json",
            "not-json",
        ],
    );
    assert_eq!(invalid.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("invalid_value"));
    let offline = invoke(
        directory.path(),
        &[
            "services",
            "call",
            "example.challenge",
            "submit",
            "--account",
            "account",
            "--json",
            "{\"answer\":\"value\"}",
        ],
    );
    assert_eq!(offline.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&offline.stderr).contains("not_running"));
}

#[test]
fn offline_status_does_not_compete_with_a_starting_writer() {
    let directory = tempfile::tempdir().unwrap();
    let writer = atri_bot::storage::Storage::open(directory.path().to_owned()).unwrap();
    let status = invoke(directory.path(), &["status"]);
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    assert_eq!(
        atri_bot::storage::Storage::open(directory.path().to_owned())
            .err()
            .unwrap()
            .code,
        "runtime_locked"
    );
    drop(writer);
}
