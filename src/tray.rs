use std::io::Cursor;
use std::sync::mpsc;

use crate::ipc;

/// Embedded PNG bytes for the tray icon
const TRAY_ICON_PNG: &[u8] = include_bytes!("../assets/tray_icon.png");

/// Load and decode the tray icon PNG, converting to ARGB32 format (big-endian)
fn load_tray_icon() -> Vec<ksni::Icon> {
    let decoder = png::Decoder::new(Cursor::new(TRAY_ICON_PNG));
    let mut reader = match decoder.read_info() {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = match reader.next_frame(&mut buf) {
        Ok(i) => i,
        Err(_) => return vec![],
    };

    let width = info.width;
    let height = info.height;

    // Verify RGBA format (4 bytes per pixel)
    if buf.len() != (width * height * 4) as usize {
        return vec![];
    }

    // Convert RGBA to ARGB32 (big-endian)
    let mut argb_data = Vec::with_capacity(buf.len());
    for chunk in buf.chunks_exact(4) {
        let r = chunk[0];
        let g = chunk[1];
        let b = chunk[2];
        let a = chunk[3];
        // ARGB32 big-endian: A, R, G, B
        let argb = ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
        argb_data.extend_from_slice(&argb.to_be_bytes());
    }

    vec![ksni::Icon {
        width: width as i32,
        height: height as i32,
        data: argb_data,
    }]
}

/// Internal tray struct implementing the `ksni::Tray` trait.
/// Holds a channel sender to forward user actions to the daemon loop.
struct PortraitTray {
    tx: mpsc::Sender<ipc::Command>,
}

impl ksni::Tray for PortraitTray {
    fn id(&self) -> String {
        "portrait".into()
    }

    fn title(&self) -> String {
        "Portrait".into()
    }

    fn icon_name(&self) -> String {
        String::new()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        load_tray_icon()
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::*;
        vec![
            StandardItem {
                label: "Toggle Camera".into(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.tx.send(ipc::Command::Toggle);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Select Webcam...".into(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.tx.send(ipc::Command::Select);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.tx.send(ipc::Command::Stop);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tx.send(ipc::Command::Toggle);
    }
}

/// Public handle for launching the system tray icon.
pub struct TrayIcon;

impl TrayIcon {
    /// Create a new `TrayIcon` with the given command sender.
    #[allow(dead_code)]
    pub fn new(_tx: mpsc::Sender<ipc::Command>) -> Self {
        TrayIcon
    }

    /// Spawn the StatusNotifierItem tray icon on a background thread.
    ///
    /// The `ksni::TrayService` runs its own D-Bus event loop in a separate
    /// thread, so this does not block the caller.
    pub fn spawn(tx: mpsc::Sender<ipc::Command>) {
        let tray = PortraitTray { tx };
        let service = ksni::TrayService::new(tray);
        service.spawn();
    }
}
