use anyhow::{Context, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use common::MachineState;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

const DEFAULT_CONTROL_PLANE_URL: &str = "http://127.0.0.1:3000";

#[derive(Debug, Serialize)]
struct LaunchMachineRequest<'a> {
    machine_name: &'a str,
    command: &'a str,
    node_id: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct MachineLaunchResponse {
    machine_id: String,
    machine_name: String,
    node_id: String,
    state: MachineState,
    command: String,
}

#[derive(Debug, Deserialize)]
struct MachineSummary {
    machine_id: String,
    machine_name: String,
    node_id: String,
    state: MachineState,
    command: String,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
}

#[derive(Debug, Deserialize)]
struct MachineMutationResponse {
    machine_id: String,
    message: String,
}

fn cli() -> Command {
    Command::new("mtc")
        .about("Manage MTC machines")
        .arg(
            Arg::new("control-plane-url")
                .long("control-plane-url")
                .env("MTC_CONTROL_PLANE_URL")
                .help("Control plane base URL")
                .default_value(DEFAULT_CONTROL_PLANE_URL)
                .global(true)
                .action(ArgAction::Set),
        )
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("launch")
                .about("launch a new machine")
                .arg(
                    Arg::new("name")
                        .help("Name of the machine/workload to launch")
                        .required(true)
                        .action(ArgAction::Set),
                )
                .arg(
                    Arg::new("command")
                        .long("command")
                        .help("Shell command to run for the machine")
                        .required(true)
                        .action(ArgAction::Set),
                )
                .arg(
                    Arg::new("node-id")
                        .long("node-id")
                        .help("Target node id")
                        .required(false)
                        .action(ArgAction::Set),
                ),
        )
        .subcommand(
            Command::new("stop")
                .about("remove a machine record from the control plane")
                .arg(
                    Arg::new("machine-id")
                        .long("machine-id")
                        .help("Target machine id")
                        .required(true)
                        .action(ArgAction::Set),
                ),
        )
        .subcommand(
            Command::new("status")
                .about("check the status of a machine")
                .arg(
                    Arg::new("machine-id")
                        .long("machine-id")
                        .help("Target machine id")
                        .required(false)
                        .action(ArgAction::Set),
                ),
        )
}

fn main() -> anyhow::Result<()> {
    let matches = cli().get_matches();
    let client = Client::new();
    let control_plane = control_plane_url(&matches);

    match matches.subcommand() {
        Some(("launch", launch_matches)) => launch_machine(&client, &control_plane, launch_matches),
        Some(("stop", stop_matches)) => stop_machine(&client, &control_plane, stop_matches),
        Some(("status", status_matches)) => show_status(&client, &control_plane, status_matches),
        _ => unreachable!(),
    }
}

fn control_plane_url(matches: &ArgMatches) -> String {
    matches
        .get_one::<String>("control-plane-url")
        .cloned()
        .unwrap_or_else(|| DEFAULT_CONTROL_PLANE_URL.to_string())
}

fn endpoint_url(control_plane: &str, path: &str) -> String {
    format!(
        "{}{}",
        control_plane.trim_end_matches('/'),
        if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        }
    )
}

fn launch_machine(
    client: &Client,
    control_plane: &str,
    matches: &ArgMatches,
) -> anyhow::Result<()> {
    let name = required_arg(matches, "name")?;
    let command = required_arg(matches, "command")?;
    let node_id = matches.get_one::<String>("node-id").map(String::as_str);

    let payload = LaunchMachineRequest {
        machine_name: name,
        command,
        node_id,
    };

    let response: MachineLaunchResponse = post_json(
        client,
        endpoint_url(control_plane, "/api/machines"),
        &payload,
    )?;

    println!("queued machine");
    println!("  id:      {}", response.machine_id);
    println!("  name:    {}", response.machine_name);
    println!("  node:    {}", response.node_id);
    println!("  state:   {:?}", response.state);
    println!("  command: {}", response.command);

    Ok(())
}

fn stop_machine(client: &Client, control_plane: &str, matches: &ArgMatches) -> anyhow::Result<()> {
    let machine_id = required_arg(matches, "machine-id")?;
    let response: MachineMutationResponse = post_empty(
        client,
        endpoint_url(
            control_plane,
            &format!("/machines/stop?machine_id={machine_id}"),
        ),
    )?;

    println!("{}", response.message);
    println!("machine_id: {}", response.machine_id);

    Ok(())
}

fn show_status(client: &Client, control_plane: &str, matches: &ArgMatches) -> anyhow::Result<()> {
    let machines: Vec<MachineSummary> =
        get_json(client, endpoint_url(control_plane, "/api/machines"))?;

    match matches.get_one::<String>("machine-id") {
        Some(machine_id) => {
            let Some(machine) = machines
                .iter()
                .find(|machine| machine.machine_id == *machine_id)
            else {
                bail!("No machine found with id={machine_id}");
            };
            print_machine_detail(machine);
        }
        None => print_machine_table(&machines),
    }

    Ok(())
}

fn required_arg<'a>(matches: &'a ArgMatches, name: &str) -> anyhow::Result<&'a str> {
    matches
        .get_one::<String>(name)
        .map(String::as_str)
        .with_context(|| format!("missing required argument '{name}'"))
}

fn post_json<T, R>(client: &Client, url: String, payload: &T) -> anyhow::Result<R>
where
    T: Serialize,
    R: for<'de> Deserialize<'de>,
{
    let response = client.post(url).json(payload).send()?;
    decode_response(response)
}

fn post_empty<R>(client: &Client, url: String) -> anyhow::Result<R>
where
    R: for<'de> Deserialize<'de>,
{
    let response = client.post(url).send()?;
    decode_response(response)
}

fn get_json<R>(client: &Client, url: String) -> anyhow::Result<R>
where
    R: for<'de> Deserialize<'de>,
{
    let response = client.get(url).send()?;
    decode_response(response)
}

fn decode_response<R>(response: reqwest::blocking::Response) -> anyhow::Result<R>
where
    R: for<'de> Deserialize<'de>,
{
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_else(|_| String::new());
        let message = if body.trim().is_empty() {
            format!("control plane returned {status}")
        } else {
            body
        };
        bail!("{message}");
    }

    response
        .json()
        .context("failed to decode control plane response")
}

fn print_machine_table(machines: &[MachineSummary]) {
    if machines.is_empty() {
        println!("No machines stored");
        return;
    }

    println!(
        "{:<36}  {:<20}  {:<20}  {:<10}  exit",
        "id", "name", "node", "state"
    );
    for machine in machines {
        println!(
            "{:<36}  {:<20}  {:<20}  {:<10}  {}",
            machine.machine_id,
            machine.machine_name,
            machine.node_id,
            format!("{:?}", machine.state),
            machine
                .exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "-".to_string())
        );
    }
}

fn print_machine_detail(machine: &MachineSummary) {
    println!("id:      {}", machine.machine_id);
    println!("name:    {}", machine.machine_name);
    println!("node:    {}", machine.node_id);
    println!("state:   {:?}", machine.state);
    println!("command: {}", machine.command);
    println!(
        "exit:    {}",
        machine
            .exit_code
            .map(|code| code.to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    println!();
    println!("stdout:");
    println!("{}", machine.stdout);
    println!();
    println!("stderr:");
    println!("{}", machine.stderr);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_accepts_global_control_plane_url() {
        let matches = cli()
            .try_get_matches_from([
                "mtc",
                "--control-plane-url",
                "http://example.test:9000",
                "status",
            ])
            .unwrap();

        assert_eq!(
            control_plane_url(&matches),
            "http://example.test:9000".to_string()
        );
    }

    #[test]
    fn cli_defaults_to_local_control_plane_url() {
        let matches = cli().try_get_matches_from(["mtc", "status"]).unwrap();

        assert_eq!(control_plane_url(&matches), "http://127.0.0.1:3000");
    }

    #[test]
    fn launch_url_targets_api_machines() {
        assert_eq!(
            endpoint_url("http://127.0.0.1:3000/", "/api/machines"),
            "http://127.0.0.1:3000/api/machines"
        );
    }

    #[test]
    fn stop_url_includes_machine_id_query() {
        assert_eq!(
            endpoint_url(
                "http://127.0.0.1:3000",
                "/machines/stop?machine_id=machine-1"
            ),
            "http://127.0.0.1:3000/machines/stop?machine_id=machine-1"
        );
    }
}
