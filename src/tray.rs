use std::sync::mpsc;

use crate::ipc;

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
        "camera-web".into()
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
