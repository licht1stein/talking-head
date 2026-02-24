use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::AppSink;
use std::time::Duration;

#[derive(Debug, Clone, serde::Serialize)]
pub struct CameraDevice {
    pub name: String,
    pub path: String,
}

pub struct CameraPipeline {
    pipeline: gst::Pipeline,
    appsink: AppSink,
    device_path: String,
    size: u32,
}

impl CameraPipeline {
    /// Build a GStreamer pipeline for the given v4l2 device at the specified size.
    pub fn new(device_path: &str, size: u32) -> Result<Self, String> {
        gst::init().map_err(|e| format!("Failed to init GStreamer: {e}"))?;

        let (pipeline, appsink) = Self::build_pipeline(device_path, size)?;

        Ok(Self {
            pipeline,
            appsink,
            device_path: device_path.to_string(),
            size,
        })
    }

    /// Return a paintable from the sink element, if available.
    ///
    /// With the appsink approach, frames are pushed to a `gtk4::Picture` via
    /// `setup_frame_callback()` instead. This returns `None`.
    pub fn paintable(&self) -> Option<gdk4::Paintable> {
        None
    }

    /// Set up a polling loop that pulls frames from the appsink and updates
    /// the given `gtk4::Picture` with a `gdk4::MemoryTexture` each frame.
    ///
    /// The closure runs on the GTK main thread via `glib::timeout_add_local`
    /// at ~30fps (~33ms interval).
    pub fn setup_frame_callback(&self, picture: &gtk4::Picture) {
        let appsink = self.appsink.clone();
        let size = self.size;
        let picture = picture.clone();

        glib::timeout_add_local(Duration::from_millis(33), move || {
            // Non-blocking pull: ClockTime::ZERO means don't wait
            if let Some(sample) = appsink.try_pull_sample(gst::ClockTime::ZERO) {
                if let Some(buffer) = sample.buffer() {
                    if let Ok(map) = buffer.map_readable() {
                        let bytes = glib::Bytes::from(map.as_slice());
                        let texture = gdk4::MemoryTexture::new(
                            size as i32,
                            size as i32,
                            gdk4::MemoryFormat::R8g8b8a8,
                            &bytes,
                            (size * 4) as usize, // stride = width * 4 bytes (RGBA)
                        );
                        picture.set_paintable(Some(&texture));
                    }
                }
            }
            glib::ControlFlow::Continue
        });
    }

    /// Set the pipeline to Playing state.
    pub fn start(&self) -> Result<(), String> {
        self.pipeline
            .set_state(gst::State::Playing)
            .map_err(|e| format!("Failed to start pipeline: {e}"))?;
        Ok(())
    }

    /// Set the pipeline to Null state.
    pub fn stop(&self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }

    /// Update the desired capture size. Takes effect on next `set_device` or rebuild.
    pub fn set_size(&mut self, size: u32) {
        self.size = size;
    }

    /// Switch to a different device (stops, rebuilds, and restarts the pipeline).
    pub fn set_device(&mut self, device_path: &str) -> Result<(), String> {
        self.stop();
        let (pipeline, appsink) = Self::build_pipeline(device_path, self.size)?;
        self.pipeline = pipeline;
        self.appsink = appsink;
        self.device_path = device_path.to_string();
        self.start()
    }

    /// Access the pipeline's bus for message handling.
    pub fn bus(&self) -> Option<gst::Bus> {
        self.pipeline.bus()
    }

    /// Borrow the underlying GStreamer pipeline.
    pub fn pipeline(&self) -> &gst::Pipeline {
        &self.pipeline
    }

    /// Enumerate available video capture devices via GStreamer DeviceMonitor.
    /// Returns an empty Vec on any error (never panics).
    pub fn list_devices() -> Vec<CameraDevice> {
        if gst::init().is_err() {
            return vec![];
        }

        let monitor = gst::DeviceMonitor::new();
        monitor.add_filter(Some("Video/Source"), None);

        if monitor.start().is_err() {
            return vec![];
        }

        let devices: Vec<CameraDevice> = monitor
            .devices()
            .iter()
            .map(|device| {
                let name = device.display_name().to_string();
                let path = device
                    .properties()
                    .and_then(|props| props.get::<String>("device.path").ok())
                    .unwrap_or_else(|| "/dev/video0".to_string());

                CameraDevice { name, path }
            })
            .collect();

        monitor.stop();
        devices
    }

    // ── internal ──────────────────────────────────────────────

    fn build_pipeline(device_path: &str, size: u32) -> Result<(gst::Pipeline, AppSink), String> {
        let pipeline_desc = format!(
            "v4l2src device={device_path} ! videoconvert ! videoscale ! \
             video/x-raw,format=RGBA,width={size},height={size} ! \
             appsink name=sink max-buffers=1 drop=true sync=false"
        );

        let element = gst::parse::launch(&pipeline_desc)
            .map_err(|e| format!("Failed to create pipeline: {e}"))?;

        let pipeline = element
            .downcast::<gst::Pipeline>()
            .map_err(|_| "parse_launch did not return a Pipeline".to_string())?;

        let sink_element = pipeline
            .by_name("sink")
            .ok_or_else(|| "Could not find sink element in pipeline".to_string())?;

        let appsink = sink_element
            .downcast::<AppSink>()
            .map_err(|_| "sink element is not an AppSink".to_string())?;

        Ok((pipeline, appsink))
    }
}

impl Drop for CameraPipeline {
    fn drop(&mut self) {
        self.stop();
    }
}
