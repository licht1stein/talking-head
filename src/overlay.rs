use gdk4::prelude::*;
use gtk4::prelude::*;
use gtk4_layer_shell::{KeyboardMode, Layer, LayerShell};

const DEFAULT_SIZE: u32 = 200;

const CSS: &str = include_str!("style.css");

pub struct OverlayWindow {
    window: gtk4::Window,
    picture: gtk4::Picture,
    size: u32,
    visible: bool,
}

impl OverlayWindow {
    pub fn new(app: &gtk4::Application) -> Self {
        // Load CSS
        let provider = gtk4::CssProvider::new();
        provider.load_from_string(CSS);
        let display = gdk4::Display::default().expect("Could not get default display");
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        // Create window
        let window = gtk4::Window::builder()
            .application(app)
            .decorated(false)
            .default_width(DEFAULT_SIZE as i32)
            .default_height(DEFAULT_SIZE as i32)
            .build();

        // Configure layer shell (must be before realize/map)
        window.init_layer_shell();
        window.set_layer(Layer::Overlay);
        window.set_exclusive_zone(-1);
        window.set_keyboard_mode(KeyboardMode::None);
        window.set_namespace(Some("portrait"));
        // No anchors = floating window (default is unanchored)

        // Add CSS class for transparent background
        window.add_css_class("portrait-overlay");

        // Create circular container + picture
        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        container.add_css_class("portrait-circle");
        container.set_halign(gtk4::Align::Center);
        container.set_valign(gtk4::Align::Center);

        let picture = gtk4::Picture::new();
        picture.set_content_fit(gtk4::ContentFit::Cover);
        picture.set_hexpand(true);
        picture.set_vexpand(true);
        container.append(&picture);

        window.set_child(Some(&container));

        // Set circular input region after window is realized (surface exists)
        let size = DEFAULT_SIZE;
        window.connect_realize(move |w| {
            set_circular_input_region(w, size);
        });

        OverlayWindow {
            window,
            picture,
            size: DEFAULT_SIZE,
            visible: false,
        }
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
        if visible {
            self.window.present();
        } else {
            self.window.set_visible(false);
        }
    }

    pub fn set_size(&mut self, size: u32) {
        self.size = size;
        let s = size as i32;
        self.window.set_default_size(s, s);

        // Update the min-size on the container via inline CSS isn't needed —
        // the window default size drives it. But we do need to update the input region.
        if let Some(surface) = self.window.surface() {
            set_circular_input_region_on_surface(&surface, size);
        }
    }

    pub fn set_paintable(&self, paintable: gdk4::Paintable) {
        self.picture.set_paintable(Some(&paintable));
    }

    pub fn window(&self) -> &gtk4::Window {
        &self.window
    }
    pub fn picture(&self) -> &gtk4::Picture {
        &self.picture
    }
}

/// Set a circular input region on the window's surface.
/// Uses small rectangles to approximate a circle so that clicks outside
/// the circle pass through to windows below.
fn set_circular_input_region(window: &gtk4::Window, size: u32) {
    if let Some(surface) = window.surface() {
        set_circular_input_region_on_surface(&surface, size);
    }
}

/// Build a circular input region from horizontal rectangle slices.
fn set_circular_input_region_on_surface(surface: &gdk4::Surface, size: u32) {
    let r = size as f64 / 2.0;
    let cx = r;
    let cy = r;

    // Approximate circle with horizontal scanline rectangles
    let mut rects = Vec::new();
    for y in 0..size as i32 {
        let dy = (y as f64 + 0.5) - cy;
        let half_width = (r * r - dy * dy).sqrt();
        if half_width <= 0.0 {
            continue;
        }
        let x0 = (cx - half_width).floor() as i32;
        let x1 = (cx + half_width).ceil() as i32;
        rects.push(gdk4::cairo::RectangleInt::new(x0, y, x1 - x0, 1));
    }

    let region = gdk4::cairo::Region::create_rectangles(&rects);
    surface.set_input_region(&region);
}
