use common::NodeId;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use tokio::process::Command;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

pub fn spawn_heartbeat(client: Client, node_id: NodeId, control_plane: String) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(5));
        let url = format!("{}/workers/heartbeat", control_plane.trim_end_matches('/'));

        loop {
            ticker.tick().await;
            debug!("Sending heartbeat for node '{}'", node_id.as_str());

            match client
                .post(&url)
                .query(&[("node_id", node_id.as_str())])
                .send()
                .await
            {
                Ok(resp) => {
                    if let Err(err) = resp.error_for_status() {
                        error!("Heartbeat got non-success status: {err}");
                    }
                }
                Err(err) => {
                    error!("Heartbeat request failed: {err}");
                }
            }
        }
    })
}

#[derive(Debug, Deserialize)]
struct MachineAssignment {
    machine_id: String,
    machine_name: String,
    command: String,
}

#[derive(Debug, Serialize)]
struct MachineClaimRequest<'a> {
    node_id: &'a NodeId,
    machine_id: &'a str,
}

#[derive(Debug, Serialize)]
struct MachineReportRequest<'a> {
    node_id: &'a NodeId,
    machine_id: &'a str,
    state: &'a str,
    exit_code: Option<i32>,
    stdout: &'a str,
    stderr: &'a str,
}

pub fn spawn_machine_runner(
    client: Client,
    node_id: NodeId,
    control_plane: String,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(2));
        let list_url = format!("{}/workers/machines", control_plane.trim_end_matches('/'));
        let claim_url = format!(
            "{}/workers/machines/claim",
            control_plane.trim_end_matches('/')
        );
        let report_url = format!(
            "{}/workers/machines/report",
            control_plane.trim_end_matches('/')
        );

        loop {
            ticker.tick().await;

            let assignments = match client
                .get(&list_url)
                .query(&[("node_id", node_id.as_str())])
                .send()
                .await
            {
                Ok(resp) => match resp.error_for_status() {
                    Ok(ok) => match ok.json::<Vec<MachineAssignment>>().await {
                        Ok(assignments) => assignments,
                        Err(err) => {
                            error!("Failed to decode machine assignments: {err}");
                            continue;
                        }
                    },
                    Err(err) => {
                        error!("Assignment poll got non-success status: {err}");
                        continue;
                    }
                },
                Err(err) => {
                    error!("Assignment poll request failed: {err}");
                    continue;
                }
            };

            for assignment in assignments {
                let claim = client
                    .post(&claim_url)
                    .json(&MachineClaimRequest {
                        node_id: &node_id,
                        machine_id: &assignment.machine_id,
                    })
                    .send()
                    .await;

                match claim {
                    Ok(resp) => {
                        if let Err(err) = resp.error_for_status() {
                            warn!(
                                "Could not claim machine '{}' for node '{}': {err}",
                                assignment.machine_id,
                                node_id.as_str()
                            );
                            continue;
                        }
                    }
                    Err(err) => {
                        error!("Failed to claim machine '{}': {err}", assignment.machine_id);
                        continue;
                    }
                }

                info!(
                    "Running machine '{}' ({}) on node '{}'",
                    assignment.machine_name,
                    assignment.machine_id,
                    node_id.as_str()
                );

                let result = run_command(&assignment.command).await;
                let report = MachineReportRequest {
                    node_id: &node_id,
                    machine_id: &assignment.machine_id,
                    state: if result.exit_code == Some(0) {
                        "Succeeded"
                    } else {
                        "Failed"
                    },
                    exit_code: result.exit_code,
                    stdout: &result.stdout,
                    stderr: &result.stderr,
                };

                match client.post(&report_url).json(&report).send().await {
                    Ok(resp) => {
                        if let Err(err) = resp.error_for_status() {
                            error!(
                                "Failed to report result for machine '{}': {err}",
                                assignment.machine_id
                            );
                        }
                    }
                    Err(err) => {
                        error!(
                            "Machine report request failed for machine '{}': {err}",
                            assignment.machine_id
                        );
                    }
                }
            }
        }
    })
}

struct CommandResult {
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
}

async fn run_command(command: &str) -> CommandResult {
    let mut process = shell_command(command);
    process.stdout(Stdio::piped()).stderr(Stdio::piped());

    match process.output().await {
        Ok(output) => CommandResult {
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        },
        Err(err) => CommandResult {
            exit_code: None,
            stdout: String::new(),
            stderr: err.to_string(),
        },
    }
}

fn shell_command(command: &str) -> Command {
    if cfg!(windows) {
        let mut process = Command::new("cmd");
        process.args(["/C", command]);
        process.env("PATH", default_command_path());
        process
    } else {
        let mut process = Command::new("/bin/sh");
        process.args(["-lc", command]);
        process.env("PATH", default_command_path());
        process
    }
}

fn default_command_path() -> &'static str {
    if cfg!(windows) {
        r"C:\Windows\System32;C:\Windows;C:\Windows\System32\Wbem"
    } else {
        "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn run_command_captures_stdout() {
        let result = run_command("printf 'hello'").await;

        assert_eq!(result.exit_code, Some(0));
        assert_eq!(result.stdout, "hello");
        assert_eq!(result.stderr, "");
    }

    #[tokio::test]
    async fn run_command_uses_default_command_path() {
        if cfg!(windows) {
            let result = run_command("where cmd").await;
            assert_eq!(result.exit_code, Some(0));
            assert!(result.stdout.to_lowercase().contains("cmd.exe"));
        } else {
            let result = run_command("command -v sh").await;
            assert_eq!(result.exit_code, Some(0));
            assert_eq!(result.stdout, "/bin/sh");
        }
    }
}
