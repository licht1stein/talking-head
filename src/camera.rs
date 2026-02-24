use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::AppSink;
use gtk4::prelude::*;
use std::time::Duration;

use crate::overlay::FrameStore;

#[derive(Debug, Clone, serde::Serialize)]
pub struct CameraDevice {
    pub name: String,
    pub path: String,
    pub max_resolution: Option<(u32, u32)>,
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

    /// Set up a polling loop that pulls frames from the appsink and writes
    /// BGRA data into the shared `frame_store`, then triggers a redraw on the
    /// `DrawingArea`.
    ///
    /// The closure runs on the GTK main thread via `glib::timeout_add_local`
    /// at ~30fps (~33ms interval).
    pub fn setup_frame_callback(
        &self,
        frame_store: FrameStore,
        drawing_area: gtk4::DrawingArea,
    ) {
        let appsink = self.appsink.clone();
        let size = self.size as i32;

        glib::timeout_add_local(Duration::from_millis(33), move || {
            // Non-blocking pull: ClockTime::ZERO means don't wait
            if let Some(sample) = appsink.try_pull_sample(gst::ClockTime::ZERO) {
                if let Some(buffer) = sample.buffer() {
                    if let Ok(map) = buffer.map_readable() {
                        *frame_store.borrow_mut() = Some((map.as_slice().to_vec(), size));
                        drawing_area.queue_draw();
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
    #[allow(dead_code)]
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

                let max_resolution = device.caps().and_then(|caps| {
                    let mut best: Option<(i64, u32, u32)> = None;
                    for s in caps.iter() {
                        let name = s.name().as_str();
                        if name != "video/x-raw" && name != "image/jpeg" { continue; }
                        let w = s.get::<i32>("width")
                            .or_else(|_| s.get::<gst::IntRange<i32>>("width").map(|r| r.max()))
                            .ok()?;
                        let h = s.get::<i32>("height")
                            .or_else(|_| s.get::<gst::IntRange<i32>>("height").map(|r| r.max()))
                            .ok()?;
                        let area = w as i64 * h as i64;
                        if best.map_or(true, |b| area > b.0) {
                            best = Some((area, w as u32, h as u32));
                        }
                    }
                    best.map(|(_, w, h)| (w, h))
                });

                CameraDevice { name, path, max_resolution }
            })
            .collect();

        monitor.stop();
        devices
    }

    // ── internal ──────────────────────────────────────────────

    /// Query the maximum supported resolution from the camera device.
    /// Returns `(width, height, gst_format_name)` where format is
    /// `"video/x-raw"` or `"image/jpeg"`, or `None` if query fails.
    fn query_max_resolution(device_path: &str) -> Option<(u32, u32, String)> {
        let monitor = gst::DeviceMonitor::new();
        monitor.add_filter(Some("Video/Source"), None);
        if monitor.start().is_err() {
            return None;
        }

        let result = monitor.devices().iter().find_map(|device| {
            let path = device
                .properties()
                .and_then(|props| props.get::<String>("device.path").ok())?;
            if path != device_path {
                return None;
            }

            let caps = device.caps()?;
            let mut best: Option<(i64, i32, i32, String)> = None;

            for s in caps.iter() {
                let format_name = s.name().as_str();
                if format_name != "video/x-raw" && format_name != "image/jpeg" {
                    continue;
                }

                let w = s.get::<i32>("width")
                    .or_else(|_| {
                        s.get::<gst::IntRange<i32>>("width").map(|r| r.max())
                    })
                    .ok()?;

                let h = s.get::<i32>("height")
                    .or_else(|_| {
                        s.get::<gst::IntRange<i32>>("height").map(|r| r.max())
                    })
                    .ok()?;

                let area = w as i64 * h as i64;
                if best.as_ref().map_or(true, |b| area > b.0) {
                    best = Some((area, w, h, format_name.to_string()));
                }
            }

            best.map(|(_, w, h, fmt)| (w as u32, h as u32, fmt))
        });

        monitor.stop();
        result
    }


    fn build_pipeline(device_path: &str, size: u32) -> Result<(gst::Pipeline, AppSink), String> {
        let src_and_decode = match Self::query_max_resolution(device_path) {
            Some((max_w, max_h, ref fmt)) if fmt == "image/jpeg" => {
                format!(
                    "v4l2src device={device_path} ! \
                     image/jpeg,width={max_w},height={max_h} ! jpegdec ! videoconvert"
                )
            }
            Some((max_w, max_h, _)) => {
                format!(
                    "v4l2src device={device_path} ! \
                     video/x-raw,width={max_w},height={max_h} ! videoconvert"
                )
            }
            None => {
                format!("v4l2src device={device_path} ! videoconvert")
            }
        };

        let pipeline_desc = format!(
            "{src_and_decode} ! \
             aspectratiocrop aspect-ratio=1/1 ! \
             videoflip method=horizontal-flip ! \
             videoscale ! video/x-raw,format=BGRA,width={size},height={size} ! \
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
