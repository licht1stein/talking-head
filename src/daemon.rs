use gstreamer as gst;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use crate::camera::CameraPipeline;
use crate::ipc::{self, Command, Response, SizeValue};
use crate::overlay::OverlayWindow;

/// Resolve a SizeValue to pixels.
fn size_to_px(val: &SizeValue) -> u32 {
    match val {
        SizeValue::Small => 128,
        SizeValue::Medium => 200,
        SizeValue::Large => 300,
        SizeValue::Custom(px) => *px,
    }
}

/// Run the portrait daemon.
///
/// This owns the GTK application, overlay window, camera pipeline, and IPC
/// dispatch loop. It blocks until the application quits (via IPC Stop command
/// or a signal).
pub fn run(device: Option<String>, size: u32, foreground: bool) {
    // Daemonize before any GTK/GLib/GStreamer init if not foreground
    if !foreground {
        nix::unistd::daemon(false, false).unwrap_or_else(|e| {
            eprintln!("portrait: failed to daemonize: {}", e);
            std::process::exit(1);
        });
    }

    // Write PID file (after fork if daemonized)
    ipc::write_pid();

    // Init GStreamer
    gst::init().unwrap_or_else(|e| {
        eprintln!("portrait: failed to init GStreamer: {}", e);
        cleanup_and_exit(1);
    });

    let app = gtk4::Application::new(None::<&str>, gio::ApplicationFlags::NON_UNIQUE);

    // Clone values for the activate closure
    let device_clone = device.clone();
    let size_val = size;

    app.connect_activate(move |app| {
        let device_path = device_clone.as_deref().unwrap_or("/dev/video0");

        // Create overlay window
        let mut overlay = OverlayWindow::new(app);
        overlay.set_size(size_val);

        // Create camera pipeline
        let camera = match CameraPipeline::new(device_path, size_val) {
            Ok(cam) => {
                if let Err(e) = cam.start() {
                    eprintln!("portrait: failed to start camera: {}", e);
                }
                // Connect appsink frames to the overlay picture
                cam.setup_frame_callback(overlay.picture());
                Some(cam)
            }
            Err(e) => {
                eprintln!("portrait: failed to create camera pipeline: {}", e);
                overlay.show_placeholder();
                None
            }
        };

        // Show overlay
        overlay.set_visible(true);

        // Shared state for the IPC dispatch closure.
        // Rc<RefCell<_>> is fine — everything runs on the GTK main thread.
        let overlay = Rc::new(RefCell::new(overlay));
        let camera = Rc::new(RefCell::new(camera));
        let visible = Rc::new(RefCell::new(true));
        let current_size = Rc::new(RefCell::new(size_val));
        let current_device = Rc::new(RefCell::new(device_path.to_string()));

        // --- GStreamer bus monitoring for camera errors/disconnect ---
        if let Some(ref cam) = *camera.borrow() {
            if let Some(bus) = cam.bus() {
                let overlay_for_bus = Rc::clone(&overlay);
                let camera_for_bus = Rc::clone(&camera);
                let device_for_bus = Rc::clone(&current_device);
                let size_for_bus = Rc::clone(&current_size);

                glib::timeout_add_local(Duration::from_millis(100), move || {
                    while let Some(msg) = bus.pop() {
                        use gstreamer::MessageView;
                        match msg.view() {
                            MessageView::Error(err) => {
                                eprintln!("portrait: camera error: {}", err.error());
                                // Stop pipeline and show placeholder
                                if let Some(ref cam) = *camera_for_bus.borrow() {
                                    cam.stop();
                                }
                                overlay_for_bus.borrow().show_placeholder();
                                // Schedule reconnect attempt
                                schedule_reconnect(
                                    Rc::clone(&camera_for_bus),
                                    Rc::clone(&overlay_for_bus),
                                    Rc::clone(&device_for_bus),
                                    Rc::clone(&size_for_bus),
                                );
                                return glib::ControlFlow::Break;
                            }
                            MessageView::Eos(_) => {
                                eprintln!("portrait: camera stream ended");
                                overlay_for_bus.borrow().show_placeholder();
                            }
                            _ => {}
                        }
                    }
                    glib::ControlFlow::Continue
                });
            }
        }

        // IPC channel: the server thread sends (Command, ResponseSender) pairs.
        // We poll from the GLib main loop via timeout_add_local.
        let (ipc_tx, ipc_rx) = mpsc::channel::<(Command, mpsc::Sender<Response>)>();

        // Start IPC server (sends commands to ipc_tx)
        if let Err(e) = ipc::start_server(ipc_tx) {
            eprintln!("portrait: failed to start IPC server: {}", e);
        }
        // Wrap receiver in Rc<RefCell> so the closure can own it
        let ipc_rx = Rc::new(RefCell::new(ipc_rx));

        // --- Tray icon ---
        // Separate channel for tray→daemon commands (no response needed).
        let (tray_tx, tray_rx) = mpsc::channel::<Command>();
        crate::tray::TrayIcon::spawn(tray_tx);

        let tray_rx = Rc::new(RefCell::new(tray_rx));

        // Poll both IPC and tray channels every 50ms from the GTK main thread
        let app_dispatch = app.clone();
        glib::timeout_add_local(Duration::from_millis(50), move || {
            // Drain pending IPC commands
            loop {
                match ipc_rx.borrow().try_recv() {
                    Ok((cmd, resp_tx)) => {
                        let response = dispatch_command(
                            &cmd,
                            &app_dispatch,
                            &overlay,
                            &camera,
                            &visible,
                            &current_size,
                            &current_device,
                        );
                        let _ = resp_tx.send(response);
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        return glib::ControlFlow::Break;
                    }
                }
            }
            // Drain pending tray commands (fire-and-forget, no response)
            loop {
                match tray_rx.borrow().try_recv() {
                    Ok(cmd) => {
                        dispatch_command(
                            &cmd,
                            &app_dispatch,
                            &overlay,
                            &camera,
                            &visible,
                            &current_size,
                            &current_device,
                        );
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => break,
                }
            }
            glib::ControlFlow::Continue
        });

        // Signal handling: SIGTERM and SIGINT
        let app_term = app.clone();
        glib::unix_signal_add_local(nix::libc::SIGTERM, move || {
            app_term.quit();
            glib::ControlFlow::Break
        });

        let app_int = app.clone();
        glib::unix_signal_add_local(nix::libc::SIGINT, move || {
            app_int.quit();
            glib::ControlFlow::Break
        });
    });

    // app.run() blocks until quit
    app.run_with_args::<&str>(&[]);

    // Cleanup on shutdown
    ipc::cleanup_pid();
    ipc::cleanup_socket();
}

/// Dispatch a single IPC command and return the response.
fn dispatch_command(
    cmd: &Command,
    app: &gtk4::Application,
    overlay: &Rc<RefCell<OverlayWindow>>,
    camera: &Rc<RefCell<Option<CameraPipeline>>>,
    visible: &Rc<RefCell<bool>>,
    size: &Rc<RefCell<u32>>,
    device: &Rc<RefCell<String>>,
) -> Response {
    match cmd {
        Command::Stop => {
            app.quit();
            Response::Ok
        }

        Command::Toggle => {
            let mut vis = visible.borrow_mut();
            *vis = !*vis;
            overlay.borrow_mut().set_visible(*vis);
            Response::Ok
        }

        Command::Status => {
            let vis = *visible.borrow();
            let sz = *size.borrow();
            let dev = device.borrow().clone();
            let (pos_x, pos_y) = overlay.borrow().get_position();
            let data = serde_json::json!({
                "running": true,
                "visible": vis,
                "device": dev,
                "size": sz,
                "position": {"x": pos_x, "y": pos_y}
            });
            Response::OkData(data)
        }

        Command::Size(val) => {
            let px = size_to_px(val);
            *size.borrow_mut() = px;
            overlay.borrow_mut().set_size(px);
            if let Some(ref mut cam) = *camera.borrow_mut() {
                cam.set_size(px);
            }
            Response::Ok
        }

        Command::Devices => {
            let devices = CameraPipeline::list_devices();
            let json = serde_json::json!(devices
                .iter()
                .map(|d| { serde_json::json!({"name": &d.name, "path": &d.path}) })
                .collect::<Vec<_>>());
            Response::OkData(json)
        }

        Command::Select => {
            let device_rc = Rc::clone(device);
            let camera_rc = Rc::clone(camera);
            let app_clone = app.clone();
            glib::idle_add_local_once(move || {
                crate::dialog::show_device_picker(&app_clone, move |path| {
                    if let Some(ref mut cam) = *camera_rc.borrow_mut() {
                        let _ = cam.set_device(&path);
                    }
                    *device_rc.borrow_mut() = path;
                });
            });
            Response::Ok
        }
    }
}

fn cleanup_and_exit(code: i32) -> ! {
    ipc::cleanup_pid();
    ipc::cleanup_socket();
    std::process::exit(code);
}

/// Show placeholder and schedule a reconnect attempt after a delay.
///
/// On success, replaces the camera in the shared state and sets up
/// frame callbacks + bus monitoring. On failure, retries after 3 more seconds.
fn schedule_reconnect(
    camera: Rc<RefCell<Option<CameraPipeline>>>,
    overlay: Rc<RefCell<OverlayWindow>>,
    device: Rc<RefCell<String>>,
    size: Rc<RefCell<u32>>,
) {
    glib::timeout_add_local_once(Duration::from_secs(3), move || {
        let device_path = device.borrow().clone();
        let size_px = *size.borrow();
        match CameraPipeline::new(&device_path, size_px) {
            Ok(cam) => {
                cam.setup_frame_callback(overlay.borrow().picture());
                if let Err(e) = cam.start() {
                    eprintln!("portrait: reconnect start failed: {}", e);
                    overlay.borrow().show_placeholder();
                    schedule_reconnect(camera, overlay, device, size);
                } else {
                    eprintln!("portrait: camera reconnected");
                    // Set up bus monitoring for the new pipeline
                    if let Some(bus) = cam.bus() {
                        let overlay_for_bus = Rc::clone(&overlay);
                        let camera_for_bus = Rc::clone(&camera);
                        let device_for_bus = Rc::clone(&device);
                        let size_for_bus = Rc::clone(&size);

                        glib::timeout_add_local(Duration::from_millis(100), move || {
                            while let Some(msg) = bus.pop() {
                                use gstreamer::MessageView;
                                match msg.view() {
                                    MessageView::Error(err) => {
                                        eprintln!("portrait: camera error: {}", err.error());
                                        if let Some(ref cam) = *camera_for_bus.borrow() {
                                            cam.stop();
                                        }
                                        overlay_for_bus.borrow().show_placeholder();
                                        schedule_reconnect(
                                            Rc::clone(&camera_for_bus),
                                            Rc::clone(&overlay_for_bus),
                                            Rc::clone(&device_for_bus),
                                            Rc::clone(&size_for_bus),
                                        );
                                        return glib::ControlFlow::Break;
                                    }
                                    MessageView::Eos(_) => {
                                        eprintln!("portrait: camera stream ended");
                                        overlay_for_bus.borrow().show_placeholder();
                                    }
                                    _ => {}
                                }
                            }
                            glib::ControlFlow::Continue
                        });
                    }
                    *camera.borrow_mut() = Some(cam);
                }
            }
            Err(e) => {
                eprintln!("portrait: reconnect failed: {}", e);
                schedule_reconnect(camera, overlay, device, size);
            }
        }
    });
}
