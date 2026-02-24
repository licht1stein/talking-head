use gstreamer as gst;
use gstreamer::prelude::*;

#[derive(Debug, Clone, serde::Serialize)]
pub struct CameraDevice {
    pub name: String,
    pub path: String,
}

pub struct CameraPipeline {
    pipeline: gst::Pipeline,
    sink: gst::Element,
    device_path: String,
    size: u32,
}

impl CameraPipeline {
    /// Build a GStreamer pipeline for the given v4l2 device at the specified size.
    pub fn new(device_path: &str, size: u32) -> Result<Self, String> {
        gst::init().map_err(|e| format!("Failed to init GStreamer: {e}"))?;

        let (pipeline, sink) = Self::build_pipeline(device_path, size)?;

        Ok(Self {
            pipeline,
            sink,
            device_path: device_path.to_string(),
            size,
        })
    }

    /// Return the paintable from the sink element, if available.
    ///
    /// Returns `None` when the sink does not expose a "paintable" property
    /// (e.g. when using autovideosink as a fallback).
    /// Task 6 will integrate the real gtk4paintablesink.
    pub fn paintable(&self) -> Option<gdk4::Paintable> {
        // The sink must expose a "paintable" property (gtk4paintablesink).
        // With autovideosink fallback this returns None.
        // Task 6 will integrate the real gtk4paintablesink with matching glib versions.
        None
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
        let (pipeline, sink) = Self::build_pipeline(device_path, self.size)?;
        self.pipeline = pipeline;
        self.sink = sink;
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

    fn build_pipeline(
        device_path: &str,
        size: u32,
    ) -> Result<(gst::Pipeline, gst::Element), String> {
        let pipeline_desc = format!(
            "v4l2src device={device_path} ! videoconvert ! videoscale ! \
             video/x-raw,width={size},height={size} ! autovideosink name=sink"
        );

        let element = gst::parse::launch(&pipeline_desc)
            .map_err(|e| format!("Failed to create pipeline: {e}"))?;

        let pipeline = element
            .downcast::<gst::Pipeline>()
            .map_err(|_| "parse_launch did not return a Pipeline".to_string())?;

        let sink = pipeline
            .by_name("sink")
            .ok_or_else(|| "Could not find sink element in pipeline".to_string())?;

        Ok((pipeline, sink))
    }
}

impl Drop for CameraPipeline {
    fn drop(&mut self) {
        self.stop();
    }
}
