// IPC module stubs for Task 4
// Full implementation in Task 2

#[derive(Debug, Clone)]
pub enum Command {
    Stop,
    Toggle,
    Status,
    Size(SizeValue),
    Select,
}

#[derive(Debug, Clone)]
pub enum SizeValue {
    Small,
    Medium,
    Large,
    Custom(u32),
}

#[derive(Debug)]
pub enum Response {
    Ok,
    OkData(serde_json::Value),
    Error(String),
}

pub fn is_already_running() -> bool {
    // TODO: Task 2 implementation
    false
}

pub fn send_command(cmd: &Command) -> Result<Response, String> {
    // TODO: Task 2 implementation
    Ok(Response::Ok)
}