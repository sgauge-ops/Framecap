use super::config::Config;
use super::metrics::MetricsAggregator;
use super::overlay::Overlay;
use super::renderer::Renderer;
use super::telemetry::{AmdGpuProvider, GpuTelemetryProvider};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[path = "loader.rs"]
pub mod loader;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TelemetryStatus {
    Available,
    Expired,
    Unsupported,
    Error,
}

#[derive(Debug, Clone)]
pub struct GpuMeasurement {
    pub value: f32,
    pub timestamp: Instant,
    pub status: TelemetryStatus,
    pub device_id: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct HardwareMetrics {
    pub cpu_temp_c: Option<f32>,
    pub gpu_vrm_temp_c: Option<f32>,
    pub cpu_vrm_temp_c: Option<f32>,
    pub gpu_edge_temp_c: Option<f32>,
    pub cpu_load_percent: Option<f32>,
    pub gpu_power_w: Option<f32>,
    pub cpu_power_w: Option<f32>,
    pub ppt_w: Option<f32>,
    pub fan_rpm: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    frametime_ms: f32,
    fps: f32,
    jitter_ms: f32,
    low_10_fps: f32,
    gpu_measurement: GpuMeasurement,
    hardware_metrics: HardwareMetrics,
    hardware_timestamp: Instant,
    timestamp: Instant,
}

impl MetricsSnapshot {
    /// Builds a snapshot and derives FPS from a valid positive frametime.
    pub fn new(
        frametime_ms: f32,
        jitter_ms: f32,
        low_10_fps: f32,
        gpu_measurement: GpuMeasurement,
        timestamp: Instant,
    ) -> Self {
        let fps = if frametime_ms.is_finite() && frametime_ms > 0.0 {
            1000.0 / frametime_ms
        } else {
            -1.0
        };
        Self::new_with_fps(
            frametime_ms,
            fps,
            jitter_ms,
            low_10_fps,
            gpu_measurement,
            timestamp,
        )
    }

    /// Builds a snapshot with an explicitly supplied FPS value.
    pub fn new_with_fps(
        frametime_ms: f32,
        fps: f32,
        jitter_ms: f32,
        low_10_fps: f32,
        gpu_measurement: GpuMeasurement,
        timestamp: Instant,
    ) -> Self {
        Self::new_with_hardware_metrics(
            frametime_ms,
            fps,
            jitter_ms,
            low_10_fps,
            gpu_measurement,
            HardwareMetrics::default(),
            timestamp,
        )
    }

    /// Builds a snapshot with hardware sensor readings in addition to GPU telemetry.
    pub fn new_with_hardware_metrics(
        frametime_ms: f32,
        fps: f32,
        jitter_ms: f32,
        low_10_fps: f32,
        gpu_measurement: GpuMeasurement,
        hardware_metrics: HardwareMetrics,
        timestamp: Instant,
    ) -> Self {
        Self::new_with_hardware_metrics_at(
            frametime_ms,
            fps,
            jitter_ms,
            low_10_fps,
            gpu_measurement,
            hardware_metrics,
            timestamp,
            timestamp,
        )
    }

    /// Builds a snapshot while retaining the hardware telemetry sample time.
    pub fn new_with_hardware_metrics_at(
        frametime_ms: f32,
        fps: f32,
        jitter_ms: f32,
        low_10_fps: f32,
        gpu_measurement: GpuMeasurement,
        hardware_metrics: HardwareMetrics,
        hardware_timestamp: Instant,
        timestamp: Instant,
    ) -> Self {
        Self {
            frametime_ms,
            fps,
            jitter_ms,
            low_10_fps,
            gpu_measurement,
            hardware_metrics,
            hardware_timestamp,
            timestamp,
        }
    }

    /// Returns the latest frametime in milliseconds.
    pub fn frametime_ms(&self) -> f32 {
        self.frametime_ms
    }

    /// Returns the FPS value stored in this snapshot.
    pub fn fps(&self) -> f32 {
        self.fps
    }

    /// Returns the measured frametime jitter in milliseconds.
    pub fn jitter_ms(&self) -> f32 {
        self.jitter_ms
    }

    /// Returns the smoothed tenth-percentile FPS value.
    pub fn low_10_fps(&self) -> f32 {
        self.low_10_fps
    }

    /// Returns the GPU measurement and its availability status.
    pub fn gpu_measurement(&self) -> &GpuMeasurement {
        &self.gpu_measurement
    }

    /// Returns the latest optional hwmon readings in degrees Celsius and watts.
    pub fn hardware_metrics(&self) -> &HardwareMetrics {
        &self.hardware_metrics
    }

    /// Returns the time at which this snapshot was created.
    pub fn timestamp(&self) -> Instant {
        self.timestamp
    }

    /// Returns the time at which the hardware telemetry was collected.
    pub fn hardware_timestamp(&self) -> Instant {
        self.hardware_timestamp
    }
}

pub struct FramecapLayer {
    pub metrics_aggregator: MetricsAggregator,
    pub telemetry_provider: Arc<dyn GpuTelemetryProvider>,
    pub overlay: Overlay,
    pub renderer: Renderer,
    pub config: Config,
    pub poll_count: u64,
}

impl FramecapLayer {
    /// Creates the high-level layer state with the requested sample window.
    pub fn new(window_size: usize) -> Self {
        // Par défaut, on utilise le fournisseur AMD (card_index 0)
        let provider = Arc::new(AmdGpuProvider::new(0));
        let aggregator = MetricsAggregator::new(window_size, Box::new(provider.clone()));

        Self {
            metrics_aggregator: aggregator,
            telemetry_provider: provider,
            overlay: Overlay::new(),
            renderer: Renderer::new(),
            config: Config::load(),
            poll_count: 0,
        }
    }

    /// Adds a presentation interval and refreshes the text overlay on schedule.
    pub fn handle_present(&mut self, interval: Duration) {
        self.poll_count += 1;
        let metrics = self.metrics_aggregator.add_sample(interval);

        if self.poll_count % self.config.refresh_rate == 0 {
            self.overlay.render(&metrics);
        }
    }
}
