mod cli;
mod ipc;
mod camera;
mod overlay;
mod daemon;
mod tray;
mod dialog;

use clap::Parser;
use cli::{Cli, Commands};

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start { device, size, foreground } => {
            // Check if already running
            if ipc::is_already_running() {
                eprintln!("portrait: daemon is already running");
                std::process::exit(1);
            }

            let size_px = Commands::parse_size(&size).unwrap_or_else(|e| {
                eprintln!("portrait: {}", e);
                std::process::exit(1);
            });

            daemon::run(device, size_px, foreground);
        }

        Commands::Stop => {
            send_or_exit(ipc::Command::Stop);
        }

        Commands::Toggle => {
            send_or_exit(ipc::Command::Toggle);
        }

        Commands::Status => {
            let response = send_command_or_exit(ipc::Command::Status);
            match response {
                ipc::Response::OkData(data) => {
                    println!("{}", serde_json::to_string_pretty(&data).unwrap());
                }
                ipc::Response::Error(e) => {
                    eprintln!("portrait: {}", e);
                    std::process::exit(1);
                }
                ipc::Response::Ok => {
                    println!("{{}}");
                }
            }
        }

        Commands::Size { value } => {
            let size_val = parse_size_value(&value);
            send_or_exit(ipc::Command::Size(size_val));
        }

        Commands::Devices => {
            // Can run without daemon — enumerate directly
            let devices = camera::CameraPipeline::list_devices();
            let json = serde_json::json!(devices.iter().map(|d| {
                serde_json::json!({"name": d.name, "path": d.path})
            }).collect::<Vec<_>>());
            println!("{}", serde_json::to_string_pretty(&json).unwrap());
        }

        Commands::Select => {
            send_or_exit(ipc::Command::Select);
        }
    }
}

fn send_or_exit(cmd: ipc::Command) {
    match ipc::send_command(&cmd) {
        Ok(ipc::Response::Ok) | Ok(ipc::Response::OkData(_)) => {}
        Ok(ipc::Response::Error(e)) => {
            eprintln!("portrait: daemon error: {}", e);
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("portrait: {}", e);
            std::process::exit(1);
        }
    }
}

fn send_command_or_exit(cmd: ipc::Command) -> ipc::Response {
    match ipc::send_command(&cmd) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("portrait: {}", e);
            std::process::exit(1);
        }
    }
}

fn parse_size_value(s: &str) -> ipc::SizeValue {
    match s.to_lowercase().as_str() {
        "small" => ipc::SizeValue::Small,
        "medium" => ipc::SizeValue::Medium,
        "large" => ipc::SizeValue::Large,
        _ => {
            let px = s.parse::<u32>().unwrap_or_else(|_| {
                eprintln!("portrait: invalid size '{}': use small/medium/large or pixels", s);
                std::process::exit(1);
            });
            ipc::SizeValue::Custom(px)
        }
    }
}