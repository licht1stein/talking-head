use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", content = "value", rename_all = "snake_case")]
pub enum Command {
    Stop,
    Toggle,
    Status,
    Size(SizeValue),
    Select,
    SelectDevice(String),
    Devices,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SizeValue {
    Small,
    Medium,
    Large,
    Custom(u32),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status", content = "data", rename_all = "snake_case")]
pub enum Response {
    Ok,
    OkData(serde_json::Value),
    Error(String),
}

/// Channel type for forwarding commands from the socket server to the daemon loop.
/// Each incoming command is paired with a one-shot sender so the handler can reply.
pub type CommandSender = mpsc::Sender<(Command, mpsc::Sender<Response>)>;

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

pub fn socket_path() -> PathBuf {
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(dir).join("portrait.sock")
}

pub fn pid_path() -> PathBuf {
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(dir).join("portrait.pid")
}

// ---------------------------------------------------------------------------
// PID management
// ---------------------------------------------------------------------------

/// Write the current process PID to the PID file.
pub fn write_pid() {
    let path = pid_path();
    let pid = std::process::id();
    std::fs::write(&path, pid.to_string()).unwrap_or_else(|e| {
        eprintln!(
            "portrait: failed to write PID file {}: {}",
            path.display(),
            e
        );
    });
}

/// Read the PID from the PID file, if it exists and contains a valid u32.
pub fn read_pid() -> Option<u32> {
    let path = pid_path();
    let content = std::fs::read_to_string(&path).ok()?;
    content.trim().parse::<u32>().ok()
}

/// Check whether a portrait daemon is already running by reading the PID file
/// and verifying the process is alive (signal 0 via `kill`).
pub fn is_already_running() -> bool {
    let pid = match read_pid() {
        Some(p) => p,
        None => return false,
    };

    // Signal 0 doesn't send a real signal — it just checks if the process exists.
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;

    match kill(Pid::from_raw(pid as i32), None::<Signal>) {
        std::result::Result::Ok(()) => true,
        Err(nix::errno::Errno::ESRCH) => {
            // Process not found — stale PID file
            cleanup_pid();
            false
        }
        Err(nix::errno::Errno::EPERM) => {
            // Process exists but we lack permission (still running)
            true
        }
        Err(_) => false,
    }
}

/// Remove the PID file.
pub fn cleanup_pid() {
    let _ = std::fs::remove_file(pid_path());
}

/// Remove the socket file.
pub fn cleanup_socket() {
    let _ = std::fs::remove_file(socket_path());
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/// Send a command to the running daemon over the Unix socket and return its response.
pub fn send_command(cmd: &Command) -> Result<Response, String> {
    let path = socket_path();

    let mut stream = UnixStream::connect(&path)
        .map_err(|e| format!("cannot connect to daemon ({}): {}", path.display(), e))?;

    let timeout = Some(Duration::from_secs(2));
    stream
        .set_read_timeout(timeout)
        .map_err(|e| format!("failed to set read timeout: {}", e))?;
    stream
        .set_write_timeout(timeout)
        .map_err(|e| format!("failed to set write timeout: {}", e))?;

    let mut payload =
        serde_json::to_string(cmd).map_err(|e| format!("failed to serialize command: {}", e))?;
    payload.push('\n');

    stream
        .write_all(payload.as_bytes())
        .map_err(|e| format!("failed to send command: {}", e))?;
    stream
        .flush()
        .map_err(|e| format!("failed to flush stream: {}", e))?;

    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| format!("failed to read response: {}", e))?;

    if line.is_empty() {
        return Err("daemon closed connection without responding".to_string());
    }

    let resp: Response =
        serde_json::from_str(line.trim()).map_err(|e| format!("invalid response JSON: {}", e))?;
    Ok(resp)
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

/// Start the IPC server on a background thread.
///
/// The server binds to `socket_path()`, accepts connections one at a time,
/// reads a JSON command per connection, forwards it through `tx`, waits for
/// the daemon's response, and writes it back.
pub fn start_server(tx: CommandSender) -> Result<(), String> {
    let path = socket_path();

    // Remove stale socket if present
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| format!("failed to remove stale socket {}: {}", path.display(), e))?;
    }

    let listener = UnixListener::bind(&path)
        .map_err(|e| format!("failed to bind socket {}: {}", path.display(), e))?;

    std::thread::Builder::new()
        .name("ipc-server".to_string())
        .spawn(move || {
            for conn in listener.incoming() {
                match conn {
                    Ok(stream) => {
                        if let Err(e) = handle_connection(&stream, &tx) {
                            eprintln!("portrait: ipc connection error: {}", e);
                        }
                    }
                    Err(e) => {
                        eprintln!("portrait: ipc accept error: {}", e);
                    }
                }
            }
        })
        .map_err(|e| format!("failed to spawn IPC server thread: {}", e))?;

    Ok(())
}

/// Handle a single client connection: read command, forward to daemon, write response.
fn handle_connection(stream: &UnixStream, tx: &CommandSender) -> Result<(), String> {
    let timeout = Some(Duration::from_secs(2));
    stream
        .set_read_timeout(timeout)
        .map_err(|e| format!("set_read_timeout: {}", e))?;
    stream
        .set_write_timeout(timeout)
        .map_err(|e| format!("set_write_timeout: {}", e))?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| format!("read command: {}", e))?;

    if line.trim().is_empty() {
        return Err("empty command".to_string());
    }

    let cmd: Command =
        serde_json::from_str(line.trim()).map_err(|e| format!("invalid command JSON: {}", e))?;

    // Create a one-shot channel for the response
    let (resp_tx, resp_rx) = mpsc::channel::<Response>();

    tx.send((cmd, resp_tx))
        .map_err(|_| "daemon command channel closed".to_string())?;

    let response = resp_rx
        .recv_timeout(Duration::from_secs(5))
        .map_err(|e| format!("waiting for daemon response: {}", e))?;

    let mut payload =
        serde_json::to_string(&response).map_err(|e| format!("serialize response: {}", e))?;
    payload.push('\n');

    // We need a mutable ref for writing; get_ref gives us &UnixStream
    let mut writer = reader.into_inner();
    writer
        .write_all(payload.as_bytes())
        .map_err(|e| format!("write response: {}", e))?;
    writer
        .flush()
        .map_err(|e| format!("flush response: {}", e))?;

    Ok(())
}
