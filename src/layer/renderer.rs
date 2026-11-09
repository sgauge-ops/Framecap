use crate::layer::config::{AutoMetric, AutoTarget, Hotkey};
use crate::layer::lib::{GpuMeasurement, HardwareMetrics, MetricsSnapshot, TelemetryStatus};
use crate::layer::settings::OverlayRenderer;
use ash::vk;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[path = "dashboard.rs"]
mod dashboard;

const FEEDBACK_VISIBLE_FOR: Duration = Duration::from_secs(1);
const FIRST_COLUMN: usize = 17;
const SECOND_COLUMN: usize = 36;
const THIRD_COLUMN: usize = 55;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum OverlayMode {
    Off = 0,
    Fps = 1,
    Full = 2,
}

pub struct OverlayModeState(AtomicU8);

pub struct OverlayRendererState(AtomicU8);

impl OverlayRendererState {
    pub fn new(renderer: OverlayRenderer) -> Self {
        Self(AtomicU8::new(match renderer {
            OverlayRenderer::Ascii => 0,
            OverlayRenderer::Modern => 1,
        }))
    }

    pub fn current(&self) -> OverlayRenderer {
        match self.0.load(Ordering::Relaxed) {
            0 => OverlayRenderer::Ascii,
            1 => OverlayRenderer::Modern,
            _ => unreachable!("invalid overlay renderer"),
        }
    }

    pub fn toggle(&self) -> OverlayRenderer {
        match self.0.fetch_xor(1, Ordering::Relaxed) {
            0 => OverlayRenderer::Modern,
            1 => OverlayRenderer::Ascii,
            _ => unreachable!("invalid overlay renderer"),
        }
    }
}

impl OverlayModeState {
    pub fn new(startup: u8) -> Self {
        Self(AtomicU8::new(startup))
    }

    pub fn mode(&self) -> OverlayMode {
        match self.0.load(Ordering::Relaxed) {
            0 => OverlayMode::Off,
            1 => OverlayMode::Fps,
            2 => OverlayMode::Full,
            value => panic!("invalid overlay mode: {value}"),
        }
    }

    pub fn cycle(&self) -> OverlayMode {
        let previous = self
            .0
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |mode| match mode {
                2 => Some(1),
                1 => Some(0),
                0 => Some(2),
                _ => None,
            })
            .expect("overlay mode must be valid");
        match previous {
            2 => OverlayMode::Fps,
            1 => OverlayMode::Off,
            0 => OverlayMode::Full,
            _ => unreachable!(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OverlayFeedbackEvent {
    HotkeyPressed(Hotkey),
    CapChanged,
    HysteresisChanged,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OverlayFeedback {
    pub hotkey: Option<Hotkey>,
    pub cap_changed: bool,
    pub hysteresis_changed: bool,
}

#[derive(Default)]
struct OverlayFeedbackTimestamps {
    hotkey: Option<(Hotkey, Instant)>,
    cap: Option<Instant>,
    hysteresis: Option<Instant>,
}

pub struct OverlayFeedbackState {
    events: parking_lot::Mutex<OverlayFeedbackTimestamps>,
}

impl OverlayFeedbackState {
    /// Creates an empty set of short-lived overlay feedback events.
    pub fn new() -> Self {
        Self {
            events: parking_lot::Mutex::new(OverlayFeedbackTimestamps::default()),
        }
    }

    /// Records a hotkey press or an effective control change.
    pub fn record(&self, event: OverlayFeedbackEvent) {
        let mut feedback = self.events.lock();
        let now = Instant::now();
        match event {
            OverlayFeedbackEvent::HotkeyPressed(hotkey) => {
                feedback.hotkey = Some((hotkey, now));
            }
            OverlayFeedbackEvent::CapChanged => feedback.cap = Some(now),
            OverlayFeedbackEvent::HysteresisChanged => feedback.hysteresis = Some(now),
        }
    }

    /// Returns feedback events that remain visible at the supplied time.
    pub fn current(&self, now: Instant) -> OverlayFeedback {
        let Some(feedback) = self.events.try_lock() else {
            return OverlayFeedback::default();
        };
        let visible =
            |timestamp: Instant| now.saturating_duration_since(timestamp) <= FEEDBACK_VISIBLE_FOR;
        OverlayFeedback {
            hotkey: feedback
                .hotkey
                .filter(|(_, timestamp)| visible(*timestamp))
                .map(|(hotkey, _)| hotkey),
            cap_changed: feedback.cap.is_some_and(visible),
            hysteresis_changed: feedback.hysteresis.is_some_and(visible),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverlayRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

pub struct Renderer {
    pub color: [u8; 4],
}

impl Renderer {
    /// Creates a text renderer with opaque white foreground text.
    pub fn new() -> Self {
        Self {
            color: [255, 255, 255, 255],
        }
    }

    fn fps_value(metrics: &MetricsSnapshot) -> String {
        let fps_value = metrics.fps();
        if fps_value.is_finite() && fps_value > 0.0 {
            format!("{fps_value:.0}")
        } else {
            "COLLECT".to_owned()
        }
    }

    /// Formats the displayed frame-rate and frame-pacing metrics.
    pub fn fps_text(metrics: &MetricsSnapshot) -> String {
        format!("{:>5} FPS", Self::fps_value(metrics))
    }

    pub fn text(metrics: &MetricsSnapshot) -> String {
        let jitter_ms = metrics.jitter_ms();
        let low_10_fps = metrics.low_10_fps();
        let jitter = if jitter_ms.is_finite() && jitter_ms >= 0.0 {
            format!("{jitter_ms:.1}")
        } else {
            "COLLECT".to_owned()
        };
        let low_10 = if low_10_fps.is_finite() && low_10_fps >= 0.0 {
            format!("{low_10_fps:.0} FPS")
        } else {
            "COLLECT".to_owned()
        };
        let mut text = format!("FPS: {:>4}", Self::fps_value(metrics));
        append_at_column(&mut text, FIRST_COLUMN, &format!("| 10% LOW: {low_10}"));
        append_at_column(&mut text, SECOND_COLUMN, &format!("| JIT: {jitter:>5} MS"));
        append_at_column(&mut text, THIRD_COLUMN, "|");
        text
    }

    /// Formats the GPU temperature, VRM temperature, utilization, and PPT row.
    pub fn gpu_sensor_text(metrics: &MetricsSnapshot) -> String {
        let hardware = metrics.hardware_metrics();
        let gpu = match metrics.gpu_measurement().status {
            TelemetryStatus::Available if metrics.gpu_measurement().value.is_finite() => {
                format!("{:.0} %", metrics.gpu_measurement().value)
            }
            TelemetryStatus::Expired => "STALE".to_owned(),
            TelemetryStatus::Error => "ERR".to_owned(),
            TelemetryStatus::Unsupported | TelemetryStatus::Available => "N/A".to_owned(),
        };
        sensor_text(
            "GPU",
            hardware.gpu_edge_temp_c,
            hardware.gpu_vrm_temp_c,
            gpu,
            "PPT",
            hardware.ppt_w,
            1,
            "W",
        )
    }

    /// Formats the CPU temperature, VRM temperature, utilization, and fan-speed row.
    pub fn cpu_sensor_text(metrics: &MetricsSnapshot) -> String {
        let hardware = metrics.hardware_metrics();
        sensor_text(
            "CPU",
            hardware.cpu_temp_c,
            hardware.cpu_vrm_temp_c,
            hardware
                .cpu_load_percent
                .filter(|load| load.is_finite())
                .map(|load| format!("{load:.0} %"))
                .unwrap_or_else(|| "N/A".to_owned()),
            "RPM",
            hardware.fan_rpm,
            0,
            "RPM",
        )
    }

    /// Formats the target row and its configured hotkey hints.
    pub fn target_text(
        target_fps: Option<f32>,
        auto_algorithm_index: Option<usize>,
        hotkeys: [Hotkey; 4],
        manual_step_fps: f32,
    ) -> String {
        let (down_hint, up_hint) = if auto_algorithm_index.is_some() {
            ("-1 %".into(), "+1 %".into())
        } else if target_fps.is_some_and(|fps| fps.is_finite() && fps > 0.0) {
            (
                format!("-{} FPS", format_fps(manual_step_fps)),
                format!("+{} FPS", format_fps(manual_step_fps)),
            )
        } else {
            ("--".into(), "--".into())
        };
        let mut text = format!("[{}] CYCLE MODE", hotkeys[0].label());
        append_at_column(
            &mut text,
            FIRST_COLUMN,
            &format!("| [{}]  {}", hotkeys[2].label(), down_hint),
        );
        append_at_column(
            &mut text,
            SECOND_COLUMN,
            &format!("| [{}]  {}", hotkeys[1].label(), up_hint),
        );
        append_at_column(
            &mut text,
            THIRD_COLUMN,
            &format!("| [{}] CYCLE OSD", hotkeys[3].label()),
        );
        text
    }

    /// Formats the current mode, cap, and active algorithm hysteresis.
    pub fn status_text(
        target_fps: Option<f32>,
        auto_algorithm_label: Option<&str>,
        hysteresis_percent: Option<f32>,
        auto_target: Option<&str>,
    ) -> String {
        let target = target_fps
            .filter(|fps| fps.is_finite() && *fps > 0.0)
            .map(format_fps)
            .unwrap_or_else(|| "OFF".to_owned());
        let mode = auto_algorithm_label.map_or_else(
            || {
                if target_fps.is_some_and(|fps| fps.is_finite() && fps > 0.0) {
                    "MANUAL".to_owned()
                } else {
                    "OFF".to_owned()
                }
            },
            |label| format!("AUTO ({label})"),
        );
        let mut text = format!("MODE: {mode}");
        append_at_column(
            &mut text,
            FIRST_COLUMN,
            &format!("| CAP: {:>6} FPS", target),
        );
        append_at_column(
            &mut text,
            SECOND_COLUMN,
            &format!(
                "| HYS: {} %",
                hysteresis_percent
                    .map(|value| format!("{value:>5.1}"))
                    .unwrap_or_else(|| "   --".to_owned())
            ),
        );
        append_at_column(
            &mut text,
            THIRD_COLUMN,
            &format!("| TARGET: {}", auto_target.unwrap_or("--")),
        );
        text
    }

    /// Returns rectangles highlighting the selected status text.
    fn feedback_rects(
        text: &str,
        extent: [u32; 2],
        scale: u32,
        top: u32,
        highlighted_text: &str,
    ) -> Vec<OverlayRect> {
        let Some(start) = text.find(highlighted_text) else {
            return Vec::new();
        };
        let left = 8u32.saturating_add((start as u32).saturating_mul(6 * scale));
        Self::text_rects_at_x(highlighted_text, extent, scale, top, left)
    }

    fn fps_value_rects(
        text: &str,
        metrics: &MetricsSnapshot,
        extent: [u32; 2],
        scale: u32,
        top: u32,
    ) -> Vec<OverlayRect> {
        Self::feedback_rects(text, extent, scale, top, &Self::fps_value(metrics))
    }

    /// Rasterizes text at the default top margin and clips it to the extent.
    pub fn text_rects(text: &str, extent: [u32; 2], scale: u32) -> Vec<OverlayRect> {
        Self::text_rects_at(text, extent, scale, 8)
    }

    /// Rasterizes text at a chosen vertical offset.
    fn text_rects_at(text: &str, extent: [u32; 2], scale: u32, top: u32) -> Vec<OverlayRect> {
        Self::text_rects_at_x(text, extent, scale, top, 8)
    }

    /// Rasterizes text at explicit coordinates, clipping glyphs to the extent.
    fn text_rects_at_x(
        text: &str,
        extent: [u32; 2],
        scale: u32,
        top: u32,
        left: u32,
    ) -> Vec<OverlayRect> {
        if scale == 0 || extent[0] <= 16 || extent[1] <= 16 {
            return Vec::new();
        }
        let mut rects = Vec::new();
        let mut cursor_x = left;
        for character in text.chars() {
            if cursor_x + 5 * scale > extent[0].saturating_sub(8) {
                break;
            }
            for (row, bits) in glyph(character).iter().copied().enumerate() {
                let cell_y = top.saturating_add(row as u32 * scale);
                if cell_y.saturating_add(scale) > extent[1] {
                    continue;
                }
                for column in 0..5 {
                    if bits & (1 << (4 - column)) != 0 {
                        rects.push(OverlayRect {
                            x: cursor_x + column * scale,
                            y: cell_y,
                            width: scale,
                            height: scale,
                        });
                    }
                }
            }
            cursor_x += 6 * scale;
        }
        rects
    }
}

fn sensor_text(
    label: &str,
    temperature_c: Option<f32>,
    vrm_temperature_c: Option<f32>,
    load_display: String,
    final_metric_label: &str,
    final_metric_value: Option<f32>,
    final_metric_precision: usize,
    final_metric_unit: &str,
) -> String {
    let mut text = format!("{label}: {:>4} °C", format_optional(temperature_c, 0));
    append_at_column(
        &mut text,
        FIRST_COLUMN,
        &format!("| VRM: {:>4} °C", format_optional(vrm_temperature_c, 0)),
    );
    append_at_column(
        &mut text,
        SECOND_COLUMN,
        &format!("| Load: {:>6}", load_display),
    );
    append_at_column(
        &mut text,
        THIRD_COLUMN,
        &format!(
            "| {final_metric_label}: {:>5} {final_metric_unit}",
            format_optional(final_metric_value, final_metric_precision)
        ),
    );
    text
}

fn format_optional(value: Option<f32>, precision: usize) -> String {
    value
        .filter(|value| value.is_finite())
        .map(|value| format!("{value:.precision$}"))
        .unwrap_or_else(|| "--".to_owned())
}

/// Pads text with spaces until it reaches the requested character column.
fn append_at_column(text: &mut String, column: usize, segment: &str) {
    let padding = column.saturating_sub(text.chars().count());
    text.push_str(&" ".repeat(padding));
    text.push_str(segment);
}

/// Formats an FPS value without unnecessary trailing zeroes.
fn format_fps(fps: f32) -> String {
    if fps.fract().abs() < 0.001 {
        format!("{fps:.0}")
    } else {
        format!("{fps:.1}")
    }
}

fn format_auto_target(target: (AutoMetric, AutoTarget), metrics: &MetricsSnapshot) -> String {
    match target {
        (AutoMetric::Fps, AutoTarget::Low10Fps) => {
            let fps = metrics.low_10_fps();
            if fps.is_finite() && fps > 0.0 {
                format!("{fps:.0} FPS")
            } else {
                "COLLECT FPS".to_owned()
            }
        }
        (AutoMetric::GpuVrmTempC, AutoTarget::Value(value)) => {
            format!("{value} °C")
        }
        (AutoMetric::FanRpm, AutoTarget::Value(value)) => {
            format!("{value} RPM")
        }
        _ => "N/A".to_owned(),
    }
}

/// Returns the seven-row bitmap for a supported overlay glyph.
fn glyph(character: char) -> [u8; 7] {
    match character {
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 16, 30, 1, 1, 30],
        '6' => [14, 16, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 1, 14],
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [14, 4, 4, 4, 4, 4, 14],
        'J' => [7, 2, 2, 2, 2, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 25, 21, 19, 17, 17, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '°' => [6, 9, 9, 6, 0, 0, 0],
        '.' => [0, 0, 0, 0, 0, 12, 12],
        ':' => [0, 12, 12, 0, 12, 12, 0],
        '|' => [4, 4, 4, 4, 4, 4, 4],
        '/' => [1, 1, 2, 4, 8, 16, 16],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '%' => [17, 2, 4, 8, 16, 17, 0],
        '(' => [2, 4, 8, 8, 8, 4, 2],
        ')' => [8, 4, 2, 2, 2, 4, 8],
        '+' => [0, 4, 4, 31, 4, 4, 0],
        '[' => [14, 8, 8, 8, 8, 8, 14],
        ']' => [14, 2, 2, 2, 2, 2, 14],
        'a' => [0, 0, 14, 1, 15, 17, 15],
        'd' => [1, 1, 15, 17, 17, 17, 15],
        'e' => [0, 0, 14, 17, 31, 16, 14],
        'f' => [6, 8, 30, 8, 8, 8, 8],
        'h' => [16, 16, 22, 25, 17, 17, 17],
        'i' => [4, 0, 12, 4, 4, 4, 14],
        'o' => [0, 0, 14, 17, 17, 17, 14],
        'p' => [0, 0, 30, 17, 30, 16, 16],
        's' => [0, 0, 15, 16, 14, 1, 30],
        't' => [8, 8, 30, 8, 8, 9, 6],
        'u' => [0, 0, 17, 17, 17, 19, 13],
        'w' => [0, 0, 17, 17, 21, 21, 10],
        'y' => [0, 0, 17, 17, 15, 1, 14],
        _ => [0; 7],
    }
}

struct ImageResources {
    image: vk::Image,
    view: vk::ImageView,
    framebuffer: vk::Framebuffer,
    signal: vk::Semaphore,
}

pub struct VulkanSwapchainRenderer {
    extent: vk::Extent2D,
    background_opacity: f32,
    renderer: Arc<OverlayRendererState>,
    fps_color: [u8; 3],
    scale_2x_min_width_px: u32,
    overlay_refresh_rate: u64,
    overlay_poll_count: u64,
    displayed_metrics: Option<MetricsSnapshot>,
    dashboard: Option<(dashboard::DashboardKey, dashboard::PreparedDashboard)>,
    render_pass: vk::RenderPass,
    background_pipeline_layout: vk::PipelineLayout,
    background_pipeline: vk::Pipeline,
    modern_pipeline_layout: vk::PipelineLayout,
    modern_pipeline: vk::Pipeline,
    images: Vec<ImageResources>,
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    queue_family: Option<u32>,
    metrics_history: PresentationMetricsHistory,
}

struct PresentationMetricsHistory {
    generation: u64,
    generation_started: Option<Instant>,
    last_present: Option<Instant>,
    fps_smoothing_window: Duration,
    minimum_collection_interval: Duration,
    fps_samples: VecDeque<(Instant, f32)>,
}

impl Default for PresentationMetricsHistory {
    /// Creates metric histories with their built-in smoothing windows.
    fn default() -> Self {
        Self {
            generation: 0,
            generation_started: None,
            last_present: None,
            fps_smoothing_window: Duration::from_secs(1),
            minimum_collection_interval: Duration::ZERO,
            fps_samples: VecDeque::new(),
        }
    }
}

impl PresentationMetricsHistory {
    /// Resets cap-dependent presentation samples.
    fn reset(&mut self, generation: u64, now: Instant) {
        self.generation = generation;
        self.generation_started = Some(now);
        self.last_present = Some(now);
        self.fps_samples.clear();
    }

    /// Records a presentation and calculates current FPS, jitter, and low-10 FPS.
    fn record(&mut self, now: Instant, generation: u64) -> (f32, f32, f32, f32) {
        if self.generation != generation || self.generation_started.is_none() {
            self.reset(generation, now);
        }

        let frametime_ms = self
            .last_present
            .replace(now)
            .map(|last| now.saturating_duration_since(last).as_secs_f32() * 1000.0)
            .unwrap_or(0.0);
        if frametime_ms.is_finite() && frametime_ms > 0.0 {
            let fps = 1000.0 / frametime_ms;
            if fps.is_finite() && fps > 0.0 {
                self.fps_samples.push_back((now, fps));
            }
        }
        while self.fps_samples.front().is_some_and(|(timestamp, _)| {
            now.saturating_duration_since(*timestamp) > self.fps_smoothing_window
        }) {
            self.fps_samples.pop_front();
        }
        let collection_ready = !self.fps_samples.is_empty()
            && self.generation_started.is_some_and(|started| {
                now.saturating_duration_since(started) >= self.minimum_collection_interval
            });
        let fps = if !collection_ready {
            -1.0
        } else {
            self.fps_samples.iter().map(|(_, fps)| fps).sum::<f32>() / self.fps_samples.len() as f32
        };
        let jitter_ms = if collection_ready {
            calculate_jitter_from_fps_samples(&self.fps_samples)
        } else {
            -1.0
        };
        let low_10_fps = if collection_ready {
            calculate_low_10_fps(&self.fps_samples)
        } else {
            -1.0
        };
        (frametime_ms, fps, jitter_ms, low_10_fps)
    }
}

/// Computes frametime jitter from the same time-windowed FPS samples as the overlay metrics.
fn calculate_jitter_from_fps_samples(samples: &VecDeque<(Instant, f32)>) -> f32 {
    if samples.is_empty() {
        return -1.0;
    }
    let mean = samples.iter().map(|(_, fps)| 1000.0 / fps).sum::<f32>() / samples.len() as f32;
    (samples
        .iter()
        .map(|(_, fps)| {
            let frametime_ms = 1000.0 / fps;
            (frametime_ms - mean).powi(2)
        })
        .sum::<f32>()
        / samples.len() as f32)
        .sqrt()
}

/// Computes the tenth-percentile FPS from time-windowed samples.
fn calculate_low_10_fps(samples: &VecDeque<(Instant, f32)>) -> f32 {
    if samples.is_empty() {
        return -1.0;
    }
    let mut fps_values = samples.iter().map(|(_, fps)| *fps).collect::<Vec<_>>();
    fps_values.sort_by(|a, b| a.total_cmp(b));
    let index = ((fps_values.len() as f64 * 0.10).ceil() as usize)
        .max(1)
        .min(fps_values.len())
        .saturating_sub(1);
    fps_values[index]
}

/// Builds the graphics pipeline used to draw the configurable overlay background.
fn create_background_pipeline(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    extent: vk::Extent2D,
    renderer: OverlayRenderer,
) -> Result<(vk::PipelineLayout, vk::Pipeline), vk::Result> {
    let (vertex_bytes, fragment_bytes, push_size): (&[u8], &[u8], u32) = match renderer {
        OverlayRenderer::Ascii => (
            include_bytes!("shaders/overlay_background.vert.spv"),
            include_bytes!("shaders/overlay_background.frag.spv"),
            20,
        ),
        OverlayRenderer::Modern => (
            include_bytes!("shaders/dashboard.vert.spv"),
            include_bytes!("shaders/dashboard.frag.spv"),
            128,
        ),
    };
    let vertex_code = ash::util::read_spv(&mut std::io::Cursor::new(vertex_bytes))
        .map_err(|_| vk::Result::ERROR_UNKNOWN)?;
    let fragment_code = ash::util::read_spv(&mut std::io::Cursor::new(fragment_bytes))
        .map_err(|_| vk::Result::ERROR_UNKNOWN)?;
    let push_constants = [vk::PushConstantRange::default()
        .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
        .offset(0)
        .size(push_size)];
    let layout_info = vk::PipelineLayoutCreateInfo::default().push_constant_ranges(&push_constants);
    let layout = unsafe { device.create_pipeline_layout(&layout_info, None)? };

    let vertex_info = vk::ShaderModuleCreateInfo::default().code(&vertex_code);
    let vertex_module = match unsafe { device.create_shader_module(&vertex_info, None) } {
        Ok(module) => module,
        Err(error) => {
            unsafe { device.destroy_pipeline_layout(layout, None) };
            return Err(error);
        }
    };
    let fragment_info = vk::ShaderModuleCreateInfo::default().code(&fragment_code);
    let fragment_module = match unsafe { device.create_shader_module(&fragment_info, None) } {
        Ok(module) => module,
        Err(error) => {
            unsafe {
                device.destroy_shader_module(vertex_module, None);
                device.destroy_pipeline_layout(layout, None);
            }
            return Err(error);
        }
    };
    let entry = c"main";
    let stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vertex_module)
            .name(entry),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(fragment_module)
            .name(entry),
    ];
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport = [vk::Viewport {
        x: 0.0,
        y: 0.0,
        width: extent.width as f32,
        height: extent.height as f32,
        min_depth: 0.0,
        max_depth: 1.0,
    }];
    let scissor = [vk::Rect2D::default().extent(extent)];
    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewports(&viewport)
        .scissors(&scissor);
    let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let blend_attachment = [vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .alpha_blend_op(vk::BlendOp::ADD)
        .color_write_mask(vk::ColorComponentFlags::RGBA)];
    let color_blend =
        vk::PipelineColorBlendStateCreateInfo::default().attachments(&blend_attachment);
    let pipeline_info = [vk::GraphicsPipelineCreateInfo::default()
        .stages(&stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization)
        .multisample_state(&multisample)
        .color_blend_state(&color_blend)
        .layout(layout)
        .render_pass(render_pass)
        .subpass(0)];
    let pipeline = match unsafe {
        device.create_graphics_pipelines(vk::PipelineCache::null(), &pipeline_info, None)
    } {
        Ok(mut pipelines) => pipelines.remove(0),
        Err((pipelines, error)) => {
            unsafe {
                for pipeline in pipelines {
                    device.destroy_pipeline(pipeline, None);
                }
                device.destroy_shader_module(fragment_module, None);
                device.destroy_shader_module(vertex_module, None);
                device.destroy_pipeline_layout(layout, None);
            }
            return Err(error);
        }
    };
    unsafe {
        device.destroy_shader_module(fragment_module, None);
        device.destroy_shader_module(vertex_module, None);
    }
    Ok((layout, pipeline))
}

impl VulkanSwapchainRenderer {
    /// Creates render resources for the images owned by a Vulkan swapchain.
    pub fn new(
        device: &ash::Device,
        format: vk::Format,
        extent: vk::Extent2D,
        swapchain_images: &[vk::Image],
        fps_smoothing_window: Duration,
        minimum_collection_interval: Duration,
        overlay_refresh_rate: u64,
        background_opacity: f32,
        renderer: Arc<OverlayRendererState>,
        fps_color: [u8; 3],
        scale_2x_min_width_px: u32,
    ) -> Result<Self, vk::Result> {
        let attachment = vk::AttachmentDescription::default()
            .format(format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        let color_reference = [vk::AttachmentReference::default()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
        let subpass = [vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_reference)];
        let render_pass_info = vk::RenderPassCreateInfo::default()
            .attachments(std::slice::from_ref(&attachment))
            .subpasses(&subpass);
        let render_pass = unsafe { device.create_render_pass(&render_pass_info, None)? };
        let (background_pipeline_layout, background_pipeline) =
            match create_background_pipeline(device, render_pass, extent, OverlayRenderer::Ascii) {
                Ok(pipeline) => pipeline,
                Err(error) => {
                    unsafe { device.destroy_render_pass(render_pass, None) };
                    return Err(error);
                }
            };
        let (modern_pipeline_layout, modern_pipeline) = match create_background_pipeline(
            device,
            render_pass,
            extent,
            OverlayRenderer::Modern,
        ) {
            Ok(pipeline) => pipeline,
            Err(error) => {
                Self::destroy_partial(
                    device,
                    render_pass,
                    background_pipeline_layout,
                    background_pipeline,
                    vk::PipelineLayout::null(),
                    vk::Pipeline::null(),
                    &[],
                );
                return Err(error);
            }
        };
        let mut images = Vec::with_capacity(swapchain_images.len());

        for &image in swapchain_images {
            let view_info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format)
                .subresource_range(
                    vk::ImageSubresourceRange::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .base_mip_level(0)
                        .level_count(1)
                        .base_array_layer(0)
                        .layer_count(1),
                );
            let view = match unsafe { device.create_image_view(&view_info, None) } {
                Ok(view) => view,
                Err(error) => {
                    Self::destroy_partial(
                        device,
                        render_pass,
                        background_pipeline_layout,
                        background_pipeline,
                        modern_pipeline_layout,
                        modern_pipeline,
                        &images,
                    );
                    return Err(error);
                }
            };
            let framebuffer_info = vk::FramebufferCreateInfo::default()
                .render_pass(render_pass)
                .attachments(std::slice::from_ref(&view))
                .width(extent.width)
                .height(extent.height)
                .layers(1);
            let framebuffer = match unsafe { device.create_framebuffer(&framebuffer_info, None) } {
                Ok(framebuffer) => framebuffer,
                Err(error) => {
                    unsafe { device.destroy_image_view(view, None) };
                    Self::destroy_partial(
                        device,
                        render_pass,
                        background_pipeline_layout,
                        background_pipeline,
                        modern_pipeline_layout,
                        modern_pipeline,
                        &images,
                    );
                    return Err(error);
                }
            };
            let signal =
                match unsafe { device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None) }
                {
                    Ok(signal) => signal,
                    Err(error) => {
                        unsafe {
                            device.destroy_framebuffer(framebuffer, None);
                            device.destroy_image_view(view, None);
                        }
                        Self::destroy_partial(
                            device,
                            render_pass,
                            background_pipeline_layout,
                            background_pipeline,
                            modern_pipeline_layout,
                            modern_pipeline,
                            &images,
                        );
                        return Err(error);
                    }
                };
            images.push(ImageResources {
                image,
                view,
                framebuffer,
                signal,
            });
        }

        Ok(Self {
            extent,
            background_opacity,
            renderer,
            fps_color,
            scale_2x_min_width_px,
            overlay_refresh_rate: overlay_refresh_rate.max(1),
            overlay_poll_count: 0,
            displayed_metrics: None,
            dashboard: None,
            render_pass,
            background_pipeline_layout,
            background_pipeline,
            modern_pipeline_layout,
            modern_pipeline,
            images,
            command_pool: vk::CommandPool::null(),
            command_buffers: Vec::new(),
            queue_family: None,
            metrics_history: PresentationMetricsHistory {
                fps_smoothing_window,
                minimum_collection_interval,
                ..PresentationMetricsHistory::default()
            },
        })
    }

    /// Releases resources created before a swapchain renderer initialization failed.
    fn destroy_partial(
        device: &ash::Device,
        render_pass: vk::RenderPass,
        background_pipeline_layout: vk::PipelineLayout,
        background_pipeline: vk::Pipeline,
        modern_pipeline_layout: vk::PipelineLayout,
        modern_pipeline: vk::Pipeline,
        images: &[ImageResources],
    ) {
        unsafe {
            device.destroy_pipeline(background_pipeline, None);
            device.destroy_pipeline_layout(background_pipeline_layout, None);
            if modern_pipeline != vk::Pipeline::null() {
                device.destroy_pipeline(modern_pipeline, None);
                device.destroy_pipeline_layout(modern_pipeline_layout, None);
            }
            for image in images {
                device.destroy_semaphore(image.signal, None);
                device.destroy_framebuffer(image.framebuffer, None);
                device.destroy_image_view(image.view, None);
            }
            device.destroy_render_pass(render_pass, None);
        }
    }

    /// Destroys all Vulkan objects owned by this swapchain renderer.
    pub fn destroy(&mut self, device: &ash::Device) {
        unsafe {
            if self.command_pool != vk::CommandPool::null() {
                device.destroy_command_pool(self.command_pool, None);
            }
            device.destroy_pipeline(self.background_pipeline, None);
            device.destroy_pipeline_layout(self.background_pipeline_layout, None);
            device.destroy_pipeline(self.modern_pipeline, None);
            device.destroy_pipeline_layout(self.modern_pipeline_layout, None);
            for image in &self.images {
                device.destroy_semaphore(image.signal, None);
                device.destroy_framebuffer(image.framebuffer, None);
                device.destroy_image_view(image.view, None);
            }
            device.destroy_render_pass(self.render_pass, None);
        }
    }

    /// Updates per-present history and returns current and refresh-cadenced metrics.
    pub fn record_present(
        &mut self,
        gpu_measurement: GpuMeasurement,
        hardware_metrics: HardwareMetrics,
        hardware_timestamp: Instant,
        cap_generation: u64,
    ) -> (MetricsSnapshot, MetricsSnapshot) {
        let now = Instant::now();
        let refresh_overlay =
            overlay_refresh_due(self.overlay_poll_count, self.overlay_refresh_rate);
        self.overlay_poll_count = self.overlay_poll_count.wrapping_add(1);
        let (frametime_ms, fps, jitter_ms, low_10_fps) =
            self.metrics_history.record(now, cap_generation);
        let current_metrics = MetricsSnapshot::new_with_hardware_metrics_at(
            frametime_ms,
            fps,
            jitter_ms,
            low_10_fps,
            gpu_measurement,
            hardware_metrics,
            hardware_timestamp,
            now,
        );
        if refresh_overlay {
            self.displayed_metrics = Some(current_metrics.clone());
        }
        let displayed_metrics = self
            .displayed_metrics
            .as_ref()
            .unwrap_or(&current_metrics)
            .clone();
        (current_metrics, displayed_metrics)
    }

    /// Records commands that draw the overlay into an acquired swapchain image.
    pub fn record_commands(
        &mut self,
        device: &ash::Device,
        queue_family: u32,
        image_index: u32,
        metrics: &MetricsSnapshot,
        overlay_mode: OverlayMode,
        target_fps: Option<f32>,
        auto_algorithm_index: Option<usize>,
        auto_algorithm_label: Option<&str>,
        hysteresis_percent: Option<f32>,
        auto_target: Option<(AutoMetric, AutoTarget)>,
        hotkeys: [Hotkey; 4],
        manual_step_fps: f32,
        feedback: OverlayFeedback,
    ) -> Result<(vk::CommandBuffer, vk::Semaphore), vk::Result> {
        if image_index as usize >= self.images.len() {
            return Err(vk::Result::ERROR_OUT_OF_DATE_KHR);
        }
        if self.queue_family != Some(queue_family) {
            if self.command_pool != vk::CommandPool::null() {
                return Err(vk::Result::ERROR_FEATURE_NOT_PRESENT);
            }
            let pool_info = vk::CommandPoolCreateInfo::default()
                .queue_family_index(queue_family)
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
            self.command_pool = unsafe { device.create_command_pool(&pool_info, None)? };
            let allocate_info = vk::CommandBufferAllocateInfo::default()
                .command_pool(self.command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(self.images.len() as u32);
            self.command_buffers = match unsafe { device.allocate_command_buffers(&allocate_info) }
            {
                Ok(buffers) => buffers,
                Err(error) => {
                    unsafe { device.destroy_command_pool(self.command_pool, None) };
                    self.command_pool = vk::CommandPool::null();
                    return Err(error);
                }
            };
            self.queue_family = Some(queue_family);
        }

        let index = image_index as usize;
        let command_buffer = self.command_buffers[index];
        unsafe {
            device.reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())?;
        }
        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe { device.begin_command_buffer(command_buffer, &begin_info)? };

        let image = self.images[index].image;
        let to_color = [vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::MEMORY_READ)
            .dst_access_mask(
                vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            )
            .old_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(1),
            )];
        unsafe {
            device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &to_color,
            );
        }

        let render_area = vk::Rect2D::default()
            .offset(vk::Offset2D { x: 0, y: 0 })
            .extent(self.extent);
        let render_begin = vk::RenderPassBeginInfo::default()
            .render_pass(self.render_pass)
            .framebuffer(self.images[index].framebuffer)
            .render_area(render_area);
        unsafe {
            device.cmd_begin_render_pass(command_buffer, &render_begin, vk::SubpassContents::INLINE)
        };

        if self.renderer.current() == OverlayRenderer::Modern {
            let key = dashboard::DashboardKey {
                timestamp: metrics.timestamp(),
                mode: overlay_mode,
                target_fps,
                auto_algorithm_index,
                auto_algorithm_label: auto_algorithm_label.map(str::to_owned),
                hysteresis_percent,
                auto_target,
                hotkeys,
                manual_step_fps,
                feedback,
            };
            if self.dashboard.as_ref().is_none_or(|(old, _)| *old != key) {
                let dashboard = dashboard::layout(
                    metrics,
                    overlay_mode,
                    target_fps,
                    auto_algorithm_index,
                    auto_algorithm_label,
                    hysteresis_percent,
                    auto_target,
                    hotkeys,
                    manual_step_fps,
                    feedback,
                    dashboard::scale_for(overlay_mode, self.extent, self.scale_2x_min_width_px),
                )
                .prepare(self.extent);
                self.dashboard = Some((key, dashboard));
            }
            let dashboard = &self
                .dashboard
                .as_ref()
                .expect("dashboard cache initialized")
                .1;
            dashboard.draw(
                device,
                command_buffer,
                self.extent,
                self.modern_pipeline,
                self.modern_pipeline_layout,
                self.background_opacity,
                self.fps_color,
            );
        } else {
            let full = overlay_mode == OverlayMode::Full;
            let text = if full {
                Renderer::text(metrics)
            } else {
                Renderer::fps_text(metrics)
            };
            let (gpu_sensor_text, cpu_sensor_text, target_text, status_text) = if full {
                let formatted_auto_target =
                    auto_target.map(|target| format_auto_target(target, metrics));
                (
                    Renderer::gpu_sensor_text(metrics),
                    Renderer::cpu_sensor_text(metrics),
                    Renderer::target_text(
                        target_fps,
                        auto_algorithm_index,
                        hotkeys,
                        manual_step_fps,
                    ),
                    Renderer::status_text(
                        target_fps,
                        auto_algorithm_label,
                        hysteresis_percent,
                        formatted_auto_target.as_deref(),
                    ),
                )
            } else {
                (String::new(), String::new(), String::new(), String::new())
            };
            let feedback = if full {
                feedback
            } else {
                OverlayFeedback::default()
            };
            let widest_line = [
                &text,
                &gpu_sensor_text,
                &cpu_sensor_text,
                &target_text,
                &status_text,
            ]
            .into_iter()
            .map(|line| line.chars().count() as u32)
            .max()
            .unwrap_or_default();
            let scale =
                overlay_text_scale(self.extent.width, self.scale_2x_min_width_px, widest_line);
            let metrics_top = 8;
            let line_height = scale;
            let line_step = 7 * scale + 3;
            let (status_top, gpu_sensor_top, cpu_sensor_top, target_top, bottom_row_top, line_tops) =
                if full {
                    let sep1_top = metrics_top + 7 * scale + 2 * scale;
                    let status_top = sep1_top + line_height + 3 * scale;
                    let sep2_top = status_top + 7 * scale + 2 * scale;
                    let gpu_sensor_top = sep2_top + line_height + 3 * scale;
                    let cpu_sensor_top = gpu_sensor_top + line_step;
                    let sep3_top = cpu_sensor_top + 7 * scale + 2 * scale;
                    let target_top = sep3_top + line_height + 3 * scale;
                    (
                        status_top,
                        gpu_sensor_top,
                        cpu_sensor_top,
                        target_top,
                        target_top,
                        vec![sep1_top, sep2_top, sep3_top],
                    )
                } else {
                    (
                        metrics_top,
                        metrics_top,
                        metrics_top,
                        metrics_top,
                        metrics_top,
                        Vec::new(),
                    )
                };
            let background_width = (widest_line * 6 * scale + 20).min(self.extent.width);
            let background_height = (bottom_row_top + 7 * scale + 8).min(self.extent.height);
            if self.background_opacity > 0.0 {
                let rect = [
                    -1.0,
                    -1.0,
                    2.0 * background_width as f32 / self.extent.width as f32 - 1.0,
                    2.0 * background_height as f32 / self.extent.height as f32 - 1.0,
                    self.background_opacity.clamp(0.0, 1.0),
                ];
                let constants = unsafe {
                    std::slice::from_raw_parts(
                        rect.as_ptr().cast::<u8>(),
                        std::mem::size_of_val(&rect),
                    )
                };
                unsafe {
                    device.cmd_bind_pipeline(
                        command_buffer,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.background_pipeline,
                    );
                    device.cmd_push_constants(
                        command_buffer,
                        self.background_pipeline_layout,
                        vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                        0,
                        constants,
                    );
                    device.cmd_draw(command_buffer, 6, 1, 0, 0);
                }
            }

            let separator_rectangles = if full {
                let line_width =
                    (widest_line * 6 * scale).min(self.extent.width.saturating_sub(16));
                line_tops
                    .into_iter()
                    .map(|top| {
                        vk::ClearRect::default()
                            .rect(
                                vk::Rect2D::default()
                                    .offset(vk::Offset2D {
                                        x: 8,
                                        y: top as i32,
                                    })
                                    .extent(vk::Extent2D {
                                        width: line_width,
                                        height: line_height,
                                    }),
                            )
                            .base_array_layer(0)
                            .layer_count(1)
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };

            let metric_rectangles = if full {
                vec![
                    (&text, metrics_top),
                    (&gpu_sensor_text, gpu_sensor_top),
                    (&cpu_sensor_text, cpu_sensor_top),
                ]
            } else {
                vec![(&text, metrics_top)]
            }
            .into_iter()
            .flat_map(|(line, top)| {
                Renderer::text_rects_at(line, [self.extent.width, self.extent.height], scale, top)
            })
            .map(|rect| {
                vk::ClearRect::default()
                    .rect(
                        vk::Rect2D::default()
                            .offset(vk::Offset2D {
                                x: rect.x as i32,
                                y: rect.y as i32,
                            })
                            .extent(vk::Extent2D {
                                width: rect.width,
                                height: rect.height,
                            }),
                    )
                    .base_array_layer(0)
                    .layer_count(1)
            })
            .collect::<Vec<_>>();
            let fps_rectangles = Renderer::fps_value_rects(
                &text,
                metrics,
                [self.extent.width, self.extent.height],
                scale,
                metrics_top,
            )
            .into_iter()
            .map(|rect| {
                vk::ClearRect::default()
                    .rect(
                        vk::Rect2D::default()
                            .offset(vk::Offset2D {
                                x: rect.x as i32,
                                y: rect.y as i32,
                            })
                            .extent(vk::Extent2D {
                                width: rect.width,
                                height: rect.height,
                            }),
                    )
                    .base_array_layer(0)
                    .layer_count(1)
            })
            .collect::<Vec<_>>();
            let target_rectangles = if full {
                Renderer::text_rects_at(
                    &target_text,
                    [self.extent.width, self.extent.height],
                    scale,
                    target_top,
                )
            } else {
                Vec::new()
            }
            .into_iter()
            .map(|rect| {
                vk::ClearRect::default()
                    .rect(
                        vk::Rect2D::default()
                            .offset(vk::Offset2D {
                                x: rect.x as i32,
                                y: rect.y as i32,
                            })
                            .extent(vk::Extent2D {
                                width: rect.width,
                                height: rect.height,
                            }),
                    )
                    .base_array_layer(0)
                    .layer_count(1)
            })
            .collect::<Vec<_>>();
            let status_rectangles = if full {
                Renderer::text_rects_at(
                    &status_text,
                    [self.extent.width, self.extent.height],
                    scale,
                    status_top,
                )
            } else {
                Vec::new()
            }
            .into_iter()
            .map(|rect| {
                vk::ClearRect::default()
                    .rect(
                        vk::Rect2D::default()
                            .offset(vk::Offset2D {
                                x: rect.x as i32,
                                y: rect.y as i32,
                            })
                            .extent(vk::Extent2D {
                                width: rect.width,
                                height: rect.height,
                            }),
                    )
                    .base_array_layer(0)
                    .layer_count(1)
            })
            .collect::<Vec<_>>();
            let target_feedback_rectangles = if full {
                feedback
                    .hotkey
                    .map(|hotkey| {
                        Renderer::feedback_rects(
                            &target_text,
                            [self.extent.width, self.extent.height],
                            scale,
                            target_top,
                            &format!("[{}]", hotkey.label()),
                        )
                    })
                    .unwrap_or_default()
            } else {
                Vec::new()
            }
            .into_iter()
            .map(|rect| {
                vk::ClearRect::default()
                    .rect(
                        vk::Rect2D::default()
                            .offset(vk::Offset2D {
                                x: rect.x as i32,
                                y: rect.y as i32,
                            })
                            .extent(vk::Extent2D {
                                width: rect.width,
                                height: rect.height,
                            }),
                    )
                    .base_array_layer(0)
                    .layer_count(1)
            })
            .collect::<Vec<_>>();
            let mut status_feedback_rectangles = Vec::new();
            if feedback.cap_changed {
                status_feedback_rectangles.extend(Renderer::feedback_rects(
                    &status_text,
                    [self.extent.width, self.extent.height],
                    scale,
                    status_top,
                    &format!(
                        "CAP: {:>6} FPS",
                        target_fps
                            .map(format_fps)
                            .unwrap_or_else(|| "OFF".to_owned())
                    ),
                ));
            }
            if feedback.hysteresis_changed {
                status_feedback_rectangles.extend(Renderer::feedback_rects(
                    &status_text,
                    [self.extent.width, self.extent.height],
                    scale,
                    status_top,
                    &format!("HYS: {:>5.1} %", hysteresis_percent.unwrap_or_default()),
                ));
            }
            let status_feedback_rectangles = status_feedback_rectangles
                .into_iter()
                .map(|rect| {
                    vk::ClearRect::default()
                        .rect(
                            vk::Rect2D::default()
                                .offset(vk::Offset2D {
                                    x: rect.x as i32,
                                    y: rect.y as i32,
                                })
                                .extent(vk::Extent2D {
                                    width: rect.width,
                                    height: rect.height,
                                }),
                        )
                        .base_array_layer(0)
                        .layer_count(1)
                })
                .collect::<Vec<_>>();
            let foreground = [vk::ClearAttachment::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .color_attachment(0)
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.92, 0.96, 0.92, 1.0],
                    },
                })];
            unsafe {
                if !separator_rectangles.is_empty() {
                    let separator_foreground = [vk::ClearAttachment::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .color_attachment(0)
                        .clear_value(vk::ClearValue {
                            color: vk::ClearColorValue {
                                float32: [0.28, 0.33, 0.39, 1.0],
                            },
                        })];
                    device.cmd_clear_attachments(
                        command_buffer,
                        &separator_foreground,
                        &separator_rectangles,
                    );
                }
                if !metric_rectangles.is_empty() {
                    device.cmd_clear_attachments(command_buffer, &foreground, &metric_rectangles);
                }
                if !fps_rectangles.is_empty() {
                    let [red, green, blue] =
                        self.fps_color.map(|channel| f32::from(channel) / 255.0);
                    let fps_foreground = [vk::ClearAttachment::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .color_attachment(0)
                        .clear_value(vk::ClearValue {
                            color: vk::ClearColorValue {
                                float32: [red, green, blue, 1.0],
                            },
                        })];
                    device.cmd_clear_attachments(command_buffer, &fps_foreground, &fps_rectangles);
                }
                let status_foreground = [vk::ClearAttachment::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .color_attachment(0)
                    .clear_value(vk::ClearValue {
                        color: vk::ClearColorValue {
                            float32: [0.35, 0.85, 1.0, 1.0],
                        },
                    })];
                let target_foreground = [vk::ClearAttachment::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .color_attachment(0)
                    .clear_value(vk::ClearValue {
                        color: vk::ClearColorValue {
                            float32: [0.35, 0.85, 1.0, 1.0],
                        },
                    })];
                if !status_rectangles.is_empty() {
                    device.cmd_clear_attachments(
                        command_buffer,
                        &status_foreground,
                        &status_rectangles,
                    );
                }
                if !target_rectangles.is_empty() {
                    device.cmd_clear_attachments(
                        command_buffer,
                        &target_foreground,
                        &target_rectangles,
                    );
                }
                let feedback_foreground = [vk::ClearAttachment::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .color_attachment(0)
                    .clear_value(vk::ClearValue {
                        color: vk::ClearColorValue {
                            float32: [1.0, 0.72, 0.20, 1.0],
                        },
                    })];
                if !target_feedback_rectangles.is_empty() {
                    device.cmd_clear_attachments(
                        command_buffer,
                        &feedback_foreground,
                        &target_feedback_rectangles,
                    );
                }
                if !status_feedback_rectangles.is_empty() {
                    device.cmd_clear_attachments(
                        command_buffer,
                        &feedback_foreground,
                        &status_feedback_rectangles,
                    );
                }
            }
        }
        unsafe { device.cmd_end_render_pass(command_buffer) };

        let to_present = [vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .dst_access_mask(vk::AccessFlags::MEMORY_READ)
            .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(1),
            )];
        unsafe {
            device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &to_present,
            );
            device.end_command_buffer(command_buffer)?;
        }
        Ok((command_buffer, self.images[index].signal))
    }

    /// Returns the signal semaphore associated with an acquired swapchain image.
    pub fn signal_for_image(&self, image_index: u32) -> Option<vk::Semaphore> {
        self.images
            .get(image_index as usize)
            .map(|image| image.signal)
    }
}

/// Determines whether displayed overlay metrics should refresh on this poll.
fn overlay_refresh_due(poll_count: u64, refresh_rate: u64) -> bool {
    poll_count % refresh_rate.max(1) == 0
}

fn overlay_text_scale(width: u32, minimum_width: u32, widest_line: u32) -> u32 {
    if width >= minimum_width && widest_line.saturating_mul(12).saturating_add(16) <= width {
        2
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOTKEYS: [Hotkey; 4] = [Hotkey::F9, Hotkey::F11, Hotkey::F10, Hotkey::F12];

    /// Builds the target row using the test hotkey layout and standard step.
    fn target_text(target_fps: Option<f32>, auto_algorithm_index: Option<usize>) -> String {
        Renderer::target_text(target_fps, auto_algorithm_index, HOTKEYS, 5.0)
    }

    fn char_column(line: &str, marker: &str) -> usize {
        line.find(marker)
            .map(|byte_index| line[..byte_index].chars().count())
            .unwrap()
    }

    #[test]
    /// Checks that metrics refresh immediately and then at the configured cadence.
    fn overlay_metrics_refresh_on_first_and_each_configured_poll() {
        let refreshed = (0..21)
            .filter(|poll| overlay_refresh_due(*poll, 10))
            .collect::<Vec<_>>();
        assert_eq!(refreshed, [0, 10, 20]);
        assert!(overlay_refresh_due(0, 0));
    }

    #[test]
    fn fps_only_text_contains_no_other_metrics() {
        let now = Instant::now();
        let metrics = MetricsSnapshot::new_with_fps(
            16.0,
            60.0,
            0.5,
            55.0,
            GpuMeasurement {
                value: 50.0,
                timestamp: now,
                status: TelemetryStatus::Available,
                device_id: None,
            },
            now,
        );
        assert_eq!(Renderer::fps_text(&metrics), "   60 FPS");
        assert!(Renderer::text(&metrics).contains("10% LOW"));
        let collecting = MetricsSnapshot::new_with_fps(
            0.0,
            -1.0,
            -1.0,
            -1.0,
            metrics.gpu_measurement().clone(),
            now,
        );
        assert_eq!(Renderer::fps_text(&collecting), "COLLECT FPS");
        for (snapshot, value, fps_column, full_column) in
            [(&metrics, "60", 3, 7), (&collecting, "COLLECT", 0, 5)]
        {
            for (text, column) in [
                (Renderer::fps_text(snapshot), fps_column),
                (Renderer::text(snapshot), full_column),
            ] {
                for scale in [1, 2] {
                    let rects = Renderer::fps_value_rects(&text, snapshot, [1200, 100], scale, 25);
                    assert!(!rects.is_empty());
                    assert_eq!(
                        rects,
                        Renderer::text_rects_at_x(
                            value,
                            [1200, 100],
                            scale,
                            25,
                            8 + column * 6 * scale,
                        )
                    );
                }
            }
        }
    }

    #[test]
    /// Checks the configured threshold and ensures doubled text remains within the swapchain.
    fn overlay_text_scale_obeys_threshold_and_fit() {
        assert_eq!(overlay_text_scale(3839, 3840, 100), 1);
        assert_eq!(overlay_text_scale(3840, 3840, 100), 2);
        assert_eq!(overlay_text_scale(3840, 3840, 320), 1);
    }

    #[test]
    /// Checks documented overlay row labels and ordering.
    fn overlay_rows_follow_the_documented_order_and_labels() {
        let manual = target_text(Some(60.0), None);
        let auto = target_text(Some(75.0), Some(1));
        let off = target_text(None, None);
        assert_eq!(
            manual,
            "[F9] CYCLE MODE  | [F10]  -5 FPS    | [F11]  +5 FPS    | [F12] CYCLE OSD"
        );
        assert_eq!(
            auto,
            "[F9] CYCLE MODE  | [F10]  -1 %      | [F11]  +1 %      | [F12] CYCLE OSD"
        );
        assert!(manual.contains("| [F10]  -5 FPS"));
        assert!(manual.contains("| [F11]  +5 FPS"));
        assert!(manual.contains("| [F12] CYCLE OSD"));
        assert!(auto.contains("| [F10]  -1 %"));
        assert!(auto.contains("| [F11]  +1 %"));
        assert!(auto.contains("| [F12] CYCLE OSD"));
        assert!(off.contains("[F10]  --") && off.contains("[F11]  --"));
        let status = Renderer::status_text(Some(60.0), None, None, None);
        assert!(status.contains("MODE: MANUAL"));
        assert!(status.contains("| CAP:     60 FPS"));
        assert!(status.contains("| HYS:    -- %"));
        let auto_status = Renderer::status_text(Some(75.0), Some("VRM"), Some(5.0), Some("100 °C"));
        assert!(auto_status.contains("MODE: AUTO (VRM)"));
        assert!(
            Renderer::status_text(Some(75.0), Some("JIT"), Some(3.0), Some("60 FPS"))
                .contains("MODE: AUTO (JIT)")
        );
        assert!(
            Renderer::status_text(Some(75.0), Some("FAN"), Some(5.0), Some("2000 RPM"))
                .contains("MODE: AUTO (FAN)")
        );
        assert!(auto_status
            .chars()
            .filter(|character| !character.is_whitespace())
            .all(|character| glyph(character) != [0; 7]));
        assert!(Renderer::status_text(None, None, None, None).contains("CAP:    OFF FPS"));
        let text = target_text(Some(60.0), None);
        assert!(text
            .chars()
            .filter(|character| !character.is_whitespace())
            .all(|character| glyph(character) != [0; 7]));
        let rects = Renderer::text_rects_at(&text, [640, 480], 1, 8);
        assert!(!rects.is_empty());
        assert!(rects.iter().all(|rect| rect.y >= 8 && rect.y < 15));
        assert!(rects.iter().all(|rect| rect.x + rect.width <= 632));

        let now = Instant::now();
        let metrics_snapshot = MetricsSnapshot::new_with_hardware_metrics(
            16.0,
            60.0,
            0.5,
            55.0,
            GpuMeasurement {
                value: 55.0,
                timestamp: now,
                status: TelemetryStatus::Available,
                device_id: None,
            },
            HardwareMetrics {
                cpu_temp_c: Some(70.0),
                gpu_vrm_temp_c: Some(57.0),
                cpu_vrm_temp_c: Some(61.0),
                gpu_edge_temp_c: Some(43.0),
                cpu_load_percent: Some(35.0),
                gpu_power_w: Some(5.8),
                cpu_power_w: Some(5.2),
                ppt_w: Some(32.61),
                fan_rpm: Some(1240.0),
            },
            now,
        );
        let metrics = Renderer::text(&metrics_snapshot);
        assert_eq!(
            metrics,
            "FPS:   60        | 10% LOW: 55 FPS  | JIT:   0.5 MS    |"
        );
        assert_eq!(
            Renderer::status_text(Some(60.0), Some("JIT"), Some(5.0), Some("60 FPS")),
            "MODE: AUTO (JIT) | CAP:     60 FPS  | HYS:   5.0 %     | TARGET: 60 FPS"
        );
        let metrics_rects = Renderer::text_rects_at(&metrics, [640, 480], 1, 17);
        assert!(metrics_rects.iter().all(|rect| rect.y >= 17 && rect.y < 31));
        let gpu_sensors = Renderer::gpu_sensor_text(&metrics_snapshot);
        let cpu_sensors = Renderer::cpu_sensor_text(&metrics_snapshot);
        assert_eq!(
            gpu_sensors,
            "GPU:   43 °C     | VRM:   57 °C     | Load:   55 %     | PPT:  32.6 W"
        );
        assert_eq!(
            cpu_sensors,
            "CPU:   70 °C     | VRM:   61 °C     | Load:   35 %     | RPM:  1240 RPM"
        );
        assert!(gpu_sensors.starts_with("GPU:   43 °C"));
        assert!(gpu_sensors.contains("| VRM:   57 °C"));
        assert!(gpu_sensors.contains("| Load:   55 %"));
        assert!(gpu_sensors.contains("| PPT:  32.6 W"));
        assert!(cpu_sensors.starts_with("CPU:   70 °C"));
        assert!(cpu_sensors.contains("| VRM:   61 °C"));
        assert!(cpu_sensors.contains("| Load:   35 %"));
        assert!(cpu_sensors.contains("| RPM:  1240 RPM"));
        assert!(gpu_sensors
            .chars()
            .filter(|character| !character.is_whitespace())
            .all(|character| glyph(character) != [0; 7]));
        for (line, marker) in [
            (&manual, "| [F10]"),
            (&status, "| CAP:"),
            (&metrics, "| 10% LOW"),
            (&gpu_sensors, "| VRM:"),
            (&cpu_sensors, "| VRM:"),
        ] {
            assert_eq!(
                char_column(line, marker),
                FIRST_COLUMN,
                "{marker:?} in {line:?}"
            );
        }
        for (line, marker) in [
            (&manual, "| [F11]"),
            (&status, "| HYS:"),
            (&gpu_sensors, "| Load:"),
            (&cpu_sensors, "| Load:"),
        ] {
            assert_eq!(char_column(line, marker), SECOND_COLUMN);
        }
        for (line, marker) in [
            (&manual, "| [F12]"),
            (&status, "| TARGET:"),
            (&gpu_sensors, "| PPT:"),
            (&cpu_sensors, "| RPM:"),
        ] {
            assert_eq!(char_column(line, marker), THIRD_COLUMN);
        }
        assert_eq!(char_column(&metrics, "| JIT:"), SECOND_COLUMN);
        assert_eq!(metrics.chars().last(), Some('|'));
        assert_eq!(metrics.chars().count() - 1, THIRD_COLUMN);
        assert!([&manual, &status, &metrics, &gpu_sensors, &cpu_sensors,]
            .into_iter()
            .all(|line| !line.trim_start().starts_with('|')));
        for unit in [" °C", " %"] {
            assert_eq!(
                char_column(&gpu_sensors, unit),
                char_column(&cpu_sensors, unit),
                "unit {unit:?} is not aligned"
            );
        }
        assert_eq!(
            char_column(&gpu_sensors, " W"),
            cpu_sensors
                .rfind(" RPM")
                .map(|index| cpu_sensors[..index].chars().count())
                .unwrap()
        );
    }

    #[test]
    fn status_row_shows_the_active_algorithm_target_and_unit() {
        let now = Instant::now();
        let gpu = GpuMeasurement {
            value: 50.0,
            timestamp: now,
            status: TelemetryStatus::Available,
            device_id: None,
        };
        let metrics = MetricsSnapshot::new_with_fps(16.0, 60.0, 0.5, 57.4, gpu.clone(), now);
        let collecting = MetricsSnapshot::new_with_fps(0.0, -1.0, -1.0, -1.0, gpu, now);
        for (label, target, expected) in [
            ("JIT", (AutoMetric::Fps, AutoTarget::Low10Fps), "57 FPS"),
            (
                "VRM",
                (AutoMetric::GpuVrmTempC, AutoTarget::Value(100.25)),
                "100.25 °C",
            ),
            (
                "FAN",
                (AutoMetric::FanRpm, AutoTarget::Value(1000.0)),
                "1000 RPM",
            ),
        ] {
            let value = format_auto_target(target, &metrics);
            assert_eq!(value, expected);
            let status = Renderer::status_text(Some(90.0), Some(label), Some(5.0), Some(&value));
            assert!(status.contains(&format!("| TARGET: {expected}")));
            assert_eq!(char_column(&status, "| TARGET:"), THIRD_COLUMN);
            assert!(status
                .chars()
                .filter(|character| !character.is_whitespace())
                .all(|character| glyph(character) != [0; 7]));
        }
        assert_eq!(
            format_auto_target((AutoMetric::Fps, AutoTarget::Low10Fps), &collecting),
            "COLLECT FPS"
        );
        for cap in [Some(90.0), None] {
            assert!(Renderer::status_text(cap, None, None, None).contains("| TARGET: --"));
        }
    }

    #[test]
    /// Checks that text columns remain anchored with the largest manual step.
    fn overlay_column_anchors_are_stable_with_maximum_manual_step() {
        let keys = Renderer::target_text(Some(240.0), None, HOTKEYS, 240.0);
        let status = Renderer::status_text(Some(240.0), None, None, None);
        assert_eq!(keys.find("| [F10]").unwrap(), FIRST_COLUMN);
        assert_eq!(keys.find("| [F11]").unwrap(), SECOND_COLUMN);
        assert_eq!(keys.find("| [F12]").unwrap(), THIRD_COLUMN);
        assert_eq!(status.find("| CAP:").unwrap(), FIRST_COLUMN);
        assert_eq!(status.find("| HYS:").unwrap(), SECOND_COLUMN);
        assert_eq!(status.find("| TARGET:").unwrap(), THIRD_COLUMN);
    }

    #[test]
    /// Checks that fractional target FPS values retain their decimal precision.
    fn target_text_preserves_fractional_fps_settings() {
        let text = Renderer::target_text(Some(77.5), None, HOTKEYS, 5.5);
        assert!(text.contains("| [F10]  -5.5 FPS"));
        assert!(text.contains("| [F11]  +5.5 FPS"));
        assert!(
            Renderer::status_text(Some(77.5), Some("JIT"), Some(5.5), Some("60 FPS"))
                .contains("HYS:   5.5 %")
        );
    }

    #[test]
    /// Checks custom key hints and highlighting of the pressed key.
    fn overlay_key_hints_follow_custom_bindings_and_highlight_the_pressed_key() {
        let hotkeys = [Hotkey::F12, Hotkey::F9, Hotkey::F10, Hotkey::F11];
        let text = Renderer::target_text(Some(60.0), None, hotkeys, 2.5);
        assert!(text.contains("[F12] CYCLE MODE"));
        assert!(text.contains("[F10]  -2.5 FPS"));
        let highlights = Renderer::feedback_rects(&text, [1200, 100], 1, 8, "[F9]");
        let f9_start = text.find("[F9]").unwrap() as u32 * 6 + 8;
        assert!(!highlights.is_empty());
        assert!(highlights
            .iter()
            .all(|rect| { rect.x >= f9_start && rect.x + rect.width <= f9_start + 24 }));
    }

    #[test]
    /// Checks that cap and hysteresis feedback highlight their status fields.
    fn cap_and_hysteresis_feedback_highlight_their_status_fields() {
        let status = Renderer::status_text(Some(62.5), Some("JIT"), Some(10.0), Some("60 FPS"));
        for field in ["CAP:   62.5 FPS", "HYS:  10.0 %"] {
            let highlights = Renderer::feedback_rects(&status, [1200, 100], 1, 17, field);
            assert!(!highlights.is_empty());
        }
    }

    #[test]
    /// Checks that cap changes restart presentation metric collection.
    fn presentation_metrics_restart_collection_after_cap_changes() {
        let start = Instant::now();
        let mut history = PresentationMetricsHistory {
            minimum_collection_interval: Duration::from_millis(100),
            ..PresentationMetricsHistory::default()
        };

        for sample in 0..=6 {
            let metrics = history.record(start + std::time::Duration::from_millis(sample * 16), 0);
            assert_eq!(metrics.1, -1.0);
            assert_eq!(metrics.2, -1.0);
            assert_eq!(metrics.3, -1.0);
        }
        let metrics = history.record(start + Duration::from_millis(112), 0);
        assert!(metrics.1 >= 0.0);
        assert!(metrics.2 >= 0.0);
        assert!(metrics.3 > 0.0);

        let reset_time = start + Duration::from_millis(128);
        let metrics = history.record(reset_time, 1);
        assert_eq!(metrics, (0.0, -1.0, -1.0, -1.0));

        for sample in 1..=6 {
            let metrics = history.record(
                reset_time + std::time::Duration::from_millis(sample * 16),
                1,
            );
            assert_eq!(metrics.1, -1.0);
            assert_eq!(metrics.2, -1.0);
            assert_eq!(metrics.3, -1.0);
        }
        let metrics = history.record(reset_time + Duration::from_millis(112), 1);
        assert!(metrics.1 > 0.0);
        assert!(metrics.2 >= 0.0);
        assert!(metrics.3 > 0.0);
    }

    #[test]
    /// Checks average FPS over the configured elapsed-time window.
    fn displayed_fps_are_averaged_over_the_configured_time_window() {
        let start = Instant::now();
        let mut history = PresentationMetricsHistory {
            fps_smoothing_window: Duration::from_secs(1),
            ..PresentationMetricsHistory::default()
        };

        assert_eq!(history.record(start, 0).1, -1.0);
        let fps = history.record(start + Duration::from_millis(10), 0).1;
        assert!((fps - 100.0).abs() < 0.1);
        let fps = history.record(start + Duration::from_millis(30), 0).1;
        assert!((fps - 75.0).abs() < 0.1);

        let fps = history.record(start + Duration::from_millis(1040), 0).1;
        assert!((fps - (1000.0 / 1010.0)).abs() < 0.01);
    }

    #[test]
    /// Checks that low-10 FPS uses the configured elapsed-time window.
    fn displayed_10_percent_low_uses_the_configured_time_window() {
        let start = Instant::now();
        let mut history = PresentationMetricsHistory {
            fps_smoothing_window: Duration::from_millis(100),
            ..PresentationMetricsHistory::default()
        };

        assert_eq!(history.record(start, 0).3, -1.0);
        let low_10 = history.record(start + Duration::from_millis(10), 0).3;
        assert!((low_10 - 100.0).abs() < 0.1);
        let low_10 = history.record(start + Duration::from_millis(30), 0).3;
        assert!((low_10 - 50.0).abs() < 0.1);

        let low_10 = history.record(start + Duration::from_millis(150), 0).3;
        assert!((low_10 - (1000.0 / 120.0)).abs() < 0.1);

        let low_10 = history.record(start + Duration::from_millis(251), 0).3;
        assert!((low_10 - (1000.0 / 101.0)).abs() < 0.1);
    }

    #[test]
    /// Checks that FPS, 10% low, and jitter share samples after the minimum collection interval.
    fn presentation_metrics_share_samples_after_the_minimum_collection_interval() {
        let start = Instant::now();
        let mut history = PresentationMetricsHistory {
            fps_smoothing_window: Duration::from_millis(100),
            minimum_collection_interval: Duration::from_millis(30),
            ..PresentationMetricsHistory::default()
        };

        assert_eq!(history.record(start, 0), (0.0, -1.0, -1.0, -1.0));
        let metrics = history.record(start + Duration::from_millis(10), 0);
        assert_eq!(metrics.1, -1.0);
        assert_eq!(metrics.2, -1.0);
        assert_eq!(metrics.3, -1.0);

        let metrics = history.record(start + Duration::from_millis(30), 0);
        assert!((metrics.1 - 75.0).abs() < 0.1);
        assert!((metrics.2 - 5.0).abs() < 0.1);
        assert!((metrics.3 - 50.0).abs() < 0.1);

        let metrics = history.record(start + Duration::from_millis(150), 0);
        assert!((metrics.1 - (1000.0 / 120.0)).abs() < 0.1);
        assert_eq!(metrics.2, 0.0);
        assert!((metrics.3 - (1000.0 / 120.0)).abs() < 0.1);
    }

    #[test]
    /// Checks overlay labels while metrics are collecting or unavailable.
    fn overlay_text_shows_collection_and_unavailable_states() {
        let now = Instant::now();
        let metrics = MetricsSnapshot::new_with_fps(
            16.67,
            -1.0,
            -1.0,
            -1.0,
            GpuMeasurement {
                value: 0.0,
                timestamp: now,
                status: TelemetryStatus::Unsupported,
                device_id: None,
            },
            now,
        );
        let text = Renderer::text(&metrics);
        assert!(text.starts_with("FPS: COLLECT"));
        assert!(text.contains("| 10% LOW: COLLECT "));
        assert!(text.contains("| JIT: COLLECT MS"));
        assert_eq!(char_column(&text, "| 10% LOW:"), FIRST_COLUMN);
        assert_eq!(char_column(&text, "| JIT:"), SECOND_COLUMN);
        assert_eq!(text.chars().last(), Some('|'));
        assert_eq!(text.chars().count() - 1, THIRD_COLUMN);
    }

    #[test]
    /// Checks visible labels for every GPU telemetry status.
    fn overlay_text_labels_all_gpu_telemetry_states() {
        let now = Instant::now();
        let text_for = |status| {
            let metrics = MetricsSnapshot::new_with_fps(
                16.67,
                60.0,
                0.5,
                55.0,
                GpuMeasurement {
                    value: 92.0,
                    timestamp: now,
                    status,
                    device_id: Some(0),
                },
                now,
            );
            Renderer::gpu_sensor_text(&metrics)
        };
        assert!(text_for(TelemetryStatus::Available).contains("Load:   92 %"));
        assert!(text_for(TelemetryStatus::Expired).contains("Load:  STALE"));
        assert!(text_for(TelemetryStatus::Unsupported).contains("Load:    N/A"));
        assert!(text_for(TelemetryStatus::Error).contains("Load:    ERR"));
    }

    #[test]
    /// Checks fixed text widths for metric and target columns.
    fn metric_and_target_columns_keep_fixed_text_widths() {
        let now = Instant::now();
        let gpu = GpuMeasurement {
            value: 95.0,
            timestamp: now,
            status: TelemetryStatus::Available,
            device_id: Some(0),
        };
        let fast = MetricsSnapshot::new_with_fps(10.0, 100.0, 0.1, 100.0, gpu.clone(), now);
        let slow = MetricsSnapshot::new_with_fps(35.0, 28.0, 8.5, 24.0, gpu, now);
        assert_eq!(Renderer::text(&fast).len(), Renderer::text(&slow).len());

        let manual_low = target_text(Some(60.0), None);
        let manual_high = target_text(Some(240.0), None);
        assert_eq!(manual_low.len(), manual_high.len());
        let status_low = Renderer::status_text(Some(1.0), None, None, None);
        let status_high = Renderer::status_text(Some(240.0), None, None, None);
        assert_eq!(status_low.len(), status_high.len());
    }

    #[test]
    /// Checks clipping of generated text rectangles to the swapchain extent.
    fn text_rects_clip_to_swapchain_extent() {
        let rects = Renderer::text_rects("FRAMECAP", [64, 32], 2);
        assert!(!rects.is_empty());
        assert!(rects.iter().all(|rect| rect.x + rect.width <= 64));
        assert!(rects.iter().all(|rect| rect.y + rect.height <= 32));
        let bottom_clipped = Renderer::text_rects_at("FRAMECAP", [64, 20], 2, 18);
        assert!(!bottom_clipped.is_empty());
        assert!(bottom_clipped.iter().all(|rect| rect.y + rect.height <= 20));
        assert!(Renderer::text_rects("FRAMECAP", [8, 8], 2).is_empty());
    }

    #[test]
    /// Checks independent expiry of the overlay feedback fields.
    fn feedback_fields_expire_after_one_second_independently() {
        let feedback = OverlayFeedbackState::new();
        let start = Instant::now();
        let later = start + Duration::from_millis(500);
        *feedback.events.lock() = OverlayFeedbackTimestamps {
            hotkey: Some((Hotkey::F10, start)),
            cap: Some(later),
            hysteresis: Some(later),
        };
        let current = feedback.current(start + FEEDBACK_VISIBLE_FOR);
        assert_eq!(current.hotkey, Some(Hotkey::F10));
        assert!(current.cap_changed && current.hysteresis_changed);
        let independently_expired =
            feedback.current(start + FEEDBACK_VISIBLE_FOR + Duration::from_millis(1));
        assert_eq!(independently_expired.hotkey, None);
        assert!(independently_expired.cap_changed && independently_expired.hysteresis_changed);
        let expired = feedback.current(later + FEEDBACK_VISIBLE_FOR + Duration::from_millis(1));
        assert_eq!(expired, OverlayFeedback::default());
    }

    #[test]
    /// Checks that the ascii overlay sections and separators are strictly ordered vertically.
    fn ascii_overlay_sections_and_separators_do_not_overlap() {
        for scale in [1, 2] {
            let metrics_top = 8;
            let line_height = scale;
            let line_step = 7 * scale + 3;
            let sep1_top = metrics_top + 7 * scale + 2 * scale;
            let status_top = sep1_top + line_height + 3 * scale;
            let sep2_top = status_top + 7 * scale + 2 * scale;
            let gpu_sensor_top = sep2_top + line_height + 3 * scale;
            let cpu_sensor_top = gpu_sensor_top + line_step;
            let sep3_top = cpu_sensor_top + 7 * scale + 2 * scale;
            let target_top = sep3_top + line_height + 3 * scale;
            let bottom_row_top = target_top;
            let background_height = bottom_row_top + 7 * scale + 8;

            assert!(metrics_top + 7 * scale < sep1_top);
            assert!(sep1_top + line_height < status_top);
            assert!(status_top + 7 * scale < sep2_top);
            assert!(sep2_top + line_height < gpu_sensor_top);
            assert!(gpu_sensor_top + 7 * scale < cpu_sensor_top);
            assert!(cpu_sensor_top + 7 * scale < sep3_top);
            assert!(sep3_top + line_height < target_top);
            assert!(target_top + 7 * scale < background_height);
        }
    }
}
