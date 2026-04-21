use clap::{Arg, ArgAction, Command};

fn cli() -> Command {
    Command::new("mtc")
        .about("Manage MTC machines")
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
                    Arg::new("node-id")
                        .long("node-id")
                        .help("Target node id")
                        .required(false)
                        .action(ArgAction::Set),
                ),
        )
        .subcommand(
            Command::new("stop").about("stop a machine").arg(
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

fn main() {
    let matches = cli().get_matches();
    match matches.subcommand() {
        Some(("launch", launch_matches)) => {
            let name = launch_matches.get_one::<String>("name").unwrap();
            let node_id = launch_matches.get_one::<String>("node-id");
            match node_id {
                Some(node_id) => println!("Launching {} on node {}", name, node_id),
                None => println!("Launching {} on auto-assigned node", name),
            }
            // Here you would add the logic to actually launch the machine
        }
        Some(("stop", stop_matches)) => {
            let machine_id = stop_matches.get_one::<String>("machine-id").unwrap();
            println!("Stopping machine with ID '{}'", machine_id);
            // Here you would add the logic to actually stop the machine
        }
        Some(("status", status_matches)) => {
            let status_machine_id = status_matches.get_one::<String>("machine-id");
            match status_machine_id {
                Some(machine_id) => println!("Checking status of machine with ID '{}'", machine_id),
                None => println!("Checking status of all machines"),
            }
        }
        _ => unreachable!(), // If all subcommands are defined above, anything else is unreachable
    }
}
