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
                eprintln!("talking-head: daemon is already running");
                std::process::exit(1);
            }

            let size_px = Commands::parse_size(&size).unwrap_or_else(|e| {
                eprintln!("talking-head: {}", e);
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
                    eprintln!("talking-head: {}", e);
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

        Commands::Install => {
            install_local();
        }
    }
}

fn send_or_exit(cmd: ipc::Command) {
    match ipc::send_command(&cmd) {
        Ok(ipc::Response::Ok) | Ok(ipc::Response::OkData(_)) => {}
        Ok(ipc::Response::Error(e)) => {
            eprintln!("talking-head: daemon error: {}", e);
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("talking-head: {}", e);
            std::process::exit(1);
        }
    }
}

fn send_command_or_exit(cmd: ipc::Command) -> ipc::Response {
    match ipc::send_command(&cmd) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("talking-head: {}", e);
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
                eprintln!("talking-head: invalid size '{}': use small/medium/large or pixels", s);
                std::process::exit(1);
            });
            ipc::SizeValue::Custom(px)
        }
    }
}

fn install_local() {
    use std::fs;
    use std::path::PathBuf;

    let home = std::env::var("HOME").unwrap_or_else(|_| {
        eprintln!("talking-head: $HOME not set");
        std::process::exit(1);
    });

    // 1. Copy binary
    let bin_dir = PathBuf::from(&home).join(".local/bin");
    fs::create_dir_all(&bin_dir).unwrap_or_else(|e| {
        eprintln!("talking-head: failed to create {}: {}", bin_dir.display(), e);
        std::process::exit(1);
    });
    let bin_dst = bin_dir.join("talking-head");
    let bin_src = std::env::current_exe().unwrap_or_else(|e| {
        eprintln!("talking-head: cannot find current binary: {}", e);
        std::process::exit(1);
    });
    fs::copy(&bin_src, &bin_dst).unwrap_or_else(|e| {
        eprintln!("talking-head: failed to copy binary: {}", e);
        std::process::exit(1);
    });
    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&bin_dst).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&bin_dst, perms).unwrap_or(());
    }
    println!("installed binary  → {}", bin_dst.display());

    // 2. Write icon
    let icon_dir = PathBuf::from(&home).join(".local/share/icons/hicolor/512x512/apps");
    fs::create_dir_all(&icon_dir).unwrap_or_else(|e| {
        eprintln!("talking-head: failed to create {}: {}", icon_dir.display(), e);
        std::process::exit(1);
    });
    let icon_dst = icon_dir.join("talking-head.png");
    const ICON_BYTES: &[u8] = include_bytes!("../assets/tray_icon.png");
    fs::write(&icon_dst, ICON_BYTES).unwrap_or_else(|e| {
        eprintln!("talking-head: failed to write icon: {}", e);
        std::process::exit(1);
    });
    println!("installed icon    → {}", icon_dst.display());

    // 3. Write .desktop file
    let apps_dir = PathBuf::from(&home).join(".local/share/applications");
    fs::create_dir_all(&apps_dir).unwrap_or_else(|e| {
        eprintln!("talking-head: failed to create {}: {}", apps_dir.display(), e);
        std::process::exit(1);
    });
    let desktop_dst = apps_dir.join("talking-head.desktop");
    let desktop_content = format!(
        "[Desktop Entry]\nVersion=1.0\nType=Application\nName=Talking Head\nComment=Webcam talking head overlay for screen recording\nExec=talking-head start\nIcon={}\nTerminal=false\nCategories=Video;Utility;\nStartupNotify=false\n",
        icon_dst.display()
    );
    fs::write(&desktop_dst, &desktop_content).unwrap_or_else(|e| {
        eprintln!("talking-head: failed to write .desktop file: {}", e);
        std::process::exit(1);
    });
    println!("installed desktop → {}", desktop_dst.display());

    println!("done. you may need to log out and back in for the launcher to pick it up.");
}