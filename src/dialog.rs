use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;

/// Open a device-picker dialog listing all available webcams.
///
/// `on_select` is called with the chosen device path when the user clicks "Apply".
/// The dialog is a plain `gtk4::Window` (not layer-shell).
pub fn show_device_picker(app: &gtk4::Application, on_select: impl Fn(String) + 'static) {
    let devices = crate::camera::CameraPipeline::list_devices();
    let paths: Rc<Vec<String>> = Rc::new(devices.iter().map(|d| d.path.clone()).collect());

    let window = gtk4::Window::builder()
        .application(app)
        .title("Select Webcam")
        .default_width(400)
        .default_height(300)
        .build();

    // --- content ---
    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    vbox.set_margin_top(12);
    vbox.set_margin_bottom(12);
    vbox.set_margin_start(12);
    vbox.set_margin_end(12);

    let list_box = gtk4::ListBox::new();
    list_box.set_selection_mode(gtk4::SelectionMode::Single);
    list_box.set_vexpand(true);

    if devices.is_empty() {
        let label = gtk4::Label::new(Some("No webcams found"));
        list_box.append(&label);
        list_box.set_selection_mode(gtk4::SelectionMode::None);
    } else {
        for device in &devices {
            let row = gtk4::ListBoxRow::new();
            let label = gtk4::Label::new(Some(&format!("{} ({})", device.name, device.path)));
            label.set_halign(gtk4::Align::Start);
            row.set_child(Some(&label));
            list_box.append(&row);
        }
        // Select the first row by default.
        if let Some(first) = list_box.row_at_index(0) {
            list_box.select_row(Some(&first));
        }
    }

    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .child(&list_box)
        .build();

    vbox.append(&scrolled);

    // --- buttons ---
    let button_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    button_box.set_halign(gtk4::Align::End);

    let cancel_btn = gtk4::Button::with_label("Cancel");
    let apply_btn = gtk4::Button::with_label("Apply");

    button_box.append(&cancel_btn);
    button_box.append(&apply_btn);
    vbox.append(&button_box);

    window.set_child(Some(&vbox));

    // --- callbacks ---
    let win_cancel = window.clone();
    cancel_btn.connect_clicked(move |_| {
        win_cancel.close();
    });

    let win_apply = window.clone();
    let paths_apply = Rc::clone(&paths);
    let list_box_apply = list_box;
    let on_select = Rc::new(RefCell::new(Some(on_select)));

    apply_btn.connect_clicked(move |_| {
        if let Some(row) = list_box_apply.selected_row() {
            let idx = row.index() as usize;
            if let Some(path) = paths_apply.get(idx) {
                if let Some(cb) = on_select.borrow_mut().take() {
                    cb(path.clone());
                }
            }
        }
        win_apply.close();
    });

    window.present();
}
