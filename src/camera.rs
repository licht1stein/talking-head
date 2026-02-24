// Camera module stubs for Task 4
// Full implementation in Task 3

#[derive(Debug, Clone)]
pub struct CameraDevice {
    pub name: String,
    pub path: String,
}

pub struct CameraPipeline;

impl CameraPipeline {
    pub fn list_devices() -> Vec<CameraDevice> {
        // TODO: Task 3 implementation
        vec![]
    }
}