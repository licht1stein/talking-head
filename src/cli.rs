use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "talking-head", about = "Webcam talking head overlay for Wayland", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Start the talking-head daemon and show the camera overlay
    Start {
        /// Webcam device path (e.g. /dev/video0)
        #[arg(long, short = 'd')]
        device: Option<String>,

        /// Initial size (small=128, medium=200, large=300, or pixels)
        #[arg(long, short = 's', default_value = "medium")]
        size: String,

        /// Run in foreground (don't daemonize)
        #[arg(long, short = 'f')]
        foreground: bool,
    },

    /// Stop the talking-head daemon
    Stop,

    /// Toggle camera overlay visibility
    Toggle,

    /// Print current daemon status as JSON
    Status,

    /// Set the size of the camera overlay
    Size {
        /// Size value: small, medium, large, or a pixel count (e.g. 250)
        value: String,
    },

    /// List available webcam devices as JSON
    Devices,

    /// Open the webcam selection dialog
    Select,

    /// Install talking-head to ~/.local (binary, icon, desktop entry)
    Install,
}

impl Commands {
    /// Parse a size string into pixels
    pub fn parse_size(s: &str) -> Result<u32, String> {
        match s.to_lowercase().as_str() {
            "small" => Ok(128),
            "medium" => Ok(200),
            "large" => Ok(300),
            _ => s.parse::<u32>().map_err(|_| {
                format!("Invalid size '{}': use small/medium/large or a pixel count", s)
            }),
        }
    }
}