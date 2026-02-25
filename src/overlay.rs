use std::cell::RefCell;
use std::rc::Rc;

use gdk4::prelude::*;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use crate::ipc;

const DEFAULT_SIZE: u32 = 200;

const CSS: &str = include_str!("style.css");

/// Shared frame buffer: (BGRA pixel data, side length in px).
pub type FrameStore = Rc<RefCell<Option<(Vec<u8>, i32)>>>;

pub struct OverlayWindow {
    window: gtk4::Window,
    drawing_area: gtk4::DrawingArea,
    frame_store: FrameStore,
    size: u32,
    visible: bool,
    position: Rc<RefCell<(i32, i32)>>,
    #[allow(dead_code)]
    drag_start: Rc<RefCell<(i32, i32)>>,
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

        // Load persisted position or use defaults
        let (init_right, init_top) = ipc::load_position().unwrap_or((20, 50));

        // Create window
        let window = gtk4::Window::builder()
            .application(app)
            .decorated(false)
            .default_width(DEFAULT_SIZE as i32)
            .default_height(DEFAULT_SIZE as i32)
            .build();

        // Configure layer shell (must be before realize/map)
        window.init_layer_shell();
        window.set_layer(Layer::Top);
        window.set_exclusive_zone(-1);
        window.set_keyboard_mode(KeyboardMode::None);
        window.set_namespace(Some("talking-head"));

        // Anchor Top+Left so margins position relative to output top-left
        // Anchor Top+Right so we position from the upper-right corner
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Right, true);
        window.set_margin(Edge::Top, init_top);
        window.set_margin(Edge::Right, init_right);

        // Add CSS class for transparent background
        window.add_css_class("talking-head-overlay");

        // Shared frame store for DrawingArea ↔ camera callback
        let frame_store: FrameStore = Rc::new(RefCell::new(None));

        // Create DrawingArea instead of Picture for manual Cairo clipping
        let drawing_area = gtk4::DrawingArea::new();
        drawing_area.set_size_request(DEFAULT_SIZE as i32, DEFAULT_SIZE as i32);

        // Draw function: clip to circle, paint video frame
        let frame_store_draw = Rc::clone(&frame_store);
        drawing_area.set_draw_func(move |_, cr, width, height| {
            // Clip to inscribed circle
            let r = (width.min(height) as f64) / 2.0;
            let cx = width as f64 / 2.0;
            let cy = height as f64 / 2.0;
            cr.arc(cx, cy, r, 0.0, 2.0 * std::f64::consts::PI);
            cr.clip();

            // Placeholder background (dark grey)
            cr.set_source_rgb(0.2, 0.2, 0.2);
            let _ = cr.paint();

            // Paint video frame if available
            if let Some((data, side)) = frame_store_draw.borrow().as_ref() {
                let side = *side;
                let stride = side * 4; // BGRA = 4 bytes/pixel
                                       // Cairo ARgb32 on little-endian = BGRA bytes in memory
                if let Ok(surface) = gdk4::cairo::ImageSurface::create_for_data(
                    data.clone(),
                    gdk4::cairo::Format::ARgb32,
                    side,
                    side,
                    stride,
                ) {
                    // Scale to widget size if needed
                    if side != width {
                        let scale = width as f64 / side as f64;
                        cr.scale(scale, scale);
                    }
                    let _ = cr.set_source_surface(&surface, 0.0, 0.0);
                    let _ = cr.paint();
                }
            }
        });

        window.set_child(Some(&drawing_area));

        // Set circular input region after window is realized (surface exists)
        let size = DEFAULT_SIZE;
        window.connect_realize(move |w| {
            set_circular_input_region(w, size);
        });

        // Shared mutable position state for drag gesture closures
        let position = Rc::new(RefCell::new((init_right, init_top)));  // (right_margin, top_margin)
        let drag_start = Rc::new(RefCell::new((0i32, 0i32)));

        // Set up drag-to-reposition gesture on the drawing area
        let gesture = gtk4::GestureDrag::new();
        gesture.set_propagation_phase(gtk4::PropagationPhase::Capture);

        let pos_for_begin = position.clone();
        let ds_for_begin = drag_start.clone();
        gesture.connect_drag_begin(move |_, _x, _y| {
            let current = *pos_for_begin.borrow();
            *ds_for_begin.borrow_mut() = current;
        });

        let pos_for_update = position.clone();
        let ds_for_update = drag_start.clone();
        let win_for_update = window.clone();
        gesture.connect_drag_update(move |_, offset_x, offset_y| {
            let (start_right, start_y) = *ds_for_update.borrow();
            let new_right = (start_right - offset_x as i32).clamp(0, 3840);
            let new_y = (start_y + offset_y as i32).clamp(0, 2160);
            *pos_for_update.borrow_mut() = (new_right, new_y);
            win_for_update.set_margin(Edge::Right, new_right);
            win_for_update.set_margin(Edge::Top, new_y);
        });

        let pos_for_end = position.clone();
        let ds_for_end = drag_start.clone();
        let win_for_end = window.clone();
        gesture.connect_drag_end(move |_, offset_x, offset_y| {
            let (start_right, start_y) = *ds_for_end.borrow();
            let new_right = (start_right - offset_x as i32).clamp(0, 3840);
            let new_y = (start_y + offset_y as i32).clamp(0, 2160);
            *pos_for_end.borrow_mut() = (new_right, new_y);
            win_for_end.set_margin(Edge::Right, new_right);
            win_for_end.set_margin(Edge::Top, new_y);
            ipc::save_position(new_right, new_y);
        });

        drawing_area.add_controller(gesture);

        OverlayWindow {
            window,
            drawing_area,
            frame_store,
            size: DEFAULT_SIZE,
            visible: false,
            position,
            drag_start,
        }
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
        if visible {
            self.window.present();
            // Restore position margins after present
            let (right, y) = *self.position.borrow();
            self.window.set_margin(Edge::Right, right);
            self.window.set_margin(Edge::Top, y);
        } else {
            self.window.set_visible(false);
        }
    }

    pub fn set_size(&mut self, size: u32) {
        self.size = size;
        let s = size as i32;
        self.window.set_default_size(s, s);
        self.drawing_area.set_size_request(s, s);

        // Update the input region
        if let Some(surface) = self.window.surface() {
            set_circular_input_region_on_surface(&surface, size);
        }
    }

    pub fn drawing_area(&self) -> &gtk4::DrawingArea {
        &self.drawing_area
    }

    pub fn frame_store(&self) -> FrameStore {
        Rc::clone(&self.frame_store)
    }

    #[allow(dead_code)]
    pub fn window(&self) -> &gtk4::Window {
        &self.window
    }

    /// Clear the frame store and redraw, showing the placeholder background.
    pub fn show_placeholder(&self) {
        *self.frame_store.borrow_mut() = None;
        self.drawing_area.queue_draw();
    }

    pub fn get_position(&self) -> (i32, i32) {
        *self.position.borrow()
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
