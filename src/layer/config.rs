use crate::layer::diagnostics;
use crate::layer::settings::{
    self, AutoAlgorithmSettings, AutoTargetSettings, MetricsSettings, OverlayRenderer,
};
use std::path::PathBuf;
use std::time::Duration;

pub const DEFAULT_TARGET_FPS: f32 = 60.0;
pub const DEFAULT_GPU_SMOOTHING_WINDOW_MS: u64 = 1000;
pub const DEFAULT_OVERLAY_BACKGROUND_OPACITY: f32 = 0.25;
pub const MIN_TARGET_FPS: f32 = 1.0;
pub const MAX_TARGET_FPS: f32 = 240.0;
pub const DEFAULT_GPU_INCREASE_LIMIT: f32 = 98.0;
pub const DEFAULT_GPU_DECREASE_LIMIT: f32 = 90.0;
pub const DEFAULT_FRAME_JITTER_LIMIT_MS: f32 = 2.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AutoMetric {
    Fps,
    GpuVrmTempC,
    FanRpm,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AutoTarget {
    Low10Fps,
    Value(f32),
}

#[derive(Clone, Debug)]
pub struct AutoAlgorithmConfig {
    pub name: String,
    pub metric: AutoMetric,
    pub target: AutoTarget,
    pub hysteresis_percent: f32,
    pub increase_interval: Duration,
    pub decrease_interval: Duration,
    pub increase_step_fps: f32,
    pub decrease_step_fps: f32,
    pub min_fps: f32,
    pub max_fps: f32,
    pub measurement_expiry: Duration,
}

impl AutoMetric {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "fps" => Some(Self::Fps),
            "gpu_vrm_temp_c" => Some(Self::GpuVrmTempC),
            "fan_rpm" => Some(Self::FanRpm),
            _ => None,
        }
    }
}

impl AutoTarget {
    fn from_settings(target: &AutoTargetSettings) -> Option<Self> {
        match target {
            AutoTargetSettings::Metric(metric) if metric == "low_10_fps" => Some(Self::Low10Fps),
            AutoTargetSettings::Value(value) if value.is_finite() => Some(Self::Value(*value)),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AutoAlgorithmKind {
    GpuThreshold,
    VrmTemp,
    FanRpm,
}

impl AutoAlgorithmKind {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "gpu_threshold" => Some(Self::GpuThreshold),
            "vrm_temp" => Some(Self::VrmTemp),
            "fan_rpm" => Some(Self::FanRpm),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Hotkey {
    F9,
    F10,
    F11,
    F12,
}

impl Hotkey {
    /// Returns the function-key label used in the overlay.
    pub fn label(self) -> &'static str {
        match self {
            Self::F9 => "F9",
            Self::F10 => "F10",
            Self::F11 => "F11",
            Self::F12 => "F12",
        }
    }
}

pub struct Config {
    pub target_fps: Option<f32>,
    pub window_size: usize,
    pub gpu_poll_rate_ms: u64,
    pub gpu_expiry_ms: u64,
    pub gpu_smoothing_window_ms: u64,
    pub refresh_rate: u64,
    pub scale_2x_min_width_px: u32,
    pub overlay_background_opacity: f32,
    pub overlay_renderer: OverlayRenderer,
    pub overlay_fps_color: [u8; 3],
    pub startup: u8,
    pub auto_algorithms: Vec<AutoAlgorithmConfig>,
    pub gpu_increase_limit: f32,
    pub gpu_decrease_limit: f32,
    pub frame_jitter_limit_ms: f32,
    pub manual_step_fps: f32,
    pub manual_min_fps: f32,
    pub manual_max_fps: f32,
    pub input_enabled: bool,
    pub hotkey_auto: Hotkey,
    pub hotkey_up: Hotkey,
    pub hotkey_down: Hotkey,
    pub hotkey_toggle: Hotkey,
    pub cpu_temp_path: Option<PathBuf>,
    pub gpu_vrm_temp_path: Option<PathBuf>,
    pub cpu_vrm_temp_path: Option<PathBuf>,
    pub gpu_edge_temp_path: Option<PathBuf>,
    pub gpu_power_path: Option<PathBuf>,
    pub cpu_power_path: Option<PathBuf>,
    pub ppt_power_path: Option<PathBuf>,
    pub fan_rpm_path: Option<PathBuf>,
    pub sensor_settings: MetricsSettings,
}

impl Config {
    /// Loads, validates, and normalizes all runtime configuration values.
    pub fn load() -> Self {
        let (file_settings, _path, _migrated) = settings::Settings::load();
        let framecap = file_settings.framecap;
        let keys = file_settings.key_bindings;
        let overlay = file_settings.overlay;
        let auto = file_settings.auto_mode;
        let manual = file_settings.manual_mode;
        let metrics = file_settings.metrics;

        let target_fps = match parse_target_fps(Some(framecap.target_fps)) {
            Ok(target_fps) => target_fps,
            Err(reason) => {
                diagnostics::error(format!(
                    "Invalid [overlay].target_fps: {reason}; frame cap disabled"
                ));
                None
            }
        };
        let window_size = positive_window_size(framecap.window_size);
        let gpu_poll_rate_ms = bounded_milliseconds(
            Some(framecap.gpu_poll_rate_ms),
            "gpu_poll_rate_ms",
            10,
            1000,
            100,
        );
        let refresh_rate = match positive_u64(overlay.refresh_rate) {
            Ok(rate) => rate,
            Err(reason) => {
                diagnostics::error(format!(
                    "Invalid [overlay].refresh_rate: {reason}; using 10"
                ));
                10
            }
        };
        let gpu_smoothing_window_ms = match positive_milliseconds(overlay.gpu_smoothing_window_ms) {
            Ok(window_ms) => window_ms,
            Err(reason) => {
                diagnostics::error(format!(
                    "Invalid [overlay].gpu_smoothing_window_ms: {reason}; using 1000 ms"
                ));
                1000
            }
        };
        let scale_2x_min_width_px = positive_u32_or_default(
            overlay.scale_2x_min_width_px,
            "scale_2x_min_width_px",
            settings::OverlaySettings::default().scale_2x_min_width_px,
        );
        let gpu_threshold_settings = auto
            .algorithms
            .iter()
            .find_map(|algorithm| algorithm.gpu_threshold.clone())
            .unwrap_or_default();
        let auto_algorithms = validated_auto_algorithms(auto.algorithms);
        let gpu_expiry_ms = auto_algorithms
            .iter()
            .map(|algorithm| algorithm.measurement_expiry.as_millis() as u64)
            .max()
            .unwrap_or(1000);
        let (gpu_increase_limit, gpu_decrease_limit) = validated_gpu_thresholds(
            gpu_threshold_settings.gpu_increase_limit,
            gpu_threshold_settings.gpu_decrease_limit,
        );
        let frame_jitter_limit_ms = bounded_finite_or_default(
            gpu_threshold_settings.frame_jitter_limit,
            "frame_jitter_limit",
            "auto-mode.algorithms.gpu_threshold",
            0.0,
            500.0,
            DEFAULT_FRAME_JITTER_LIMIT_MS,
        );
        let background_opacity = bounded_finite_or_default(
            overlay.background_opacity,
            "background_opacity",
            "overlay",
            0.0,
            1.0,
            DEFAULT_OVERLAY_BACKGROUND_OPACITY,
        );
        let startup = positive_u8(overlay.startup, "startup", 0..=2, 2);

        let manual_step_fps = positive_finite_or_default(
            manual.manual_step_fps,
            "manual_step_fps",
            "manual-mode",
            5.0,
        );
        let (manual_min_fps, manual_max_fps) =
            validated_fps_bounds(manual.manual_min_fps, manual.manual_max_fps, "manual-mode");

        Self {
            target_fps,
            window_size,
            gpu_poll_rate_ms,
            refresh_rate,
            gpu_expiry_ms,
            gpu_smoothing_window_ms,
            scale_2x_min_width_px,
            auto_algorithms,
            gpu_increase_limit,
            gpu_decrease_limit,
            frame_jitter_limit_ms,
            overlay_background_opacity: background_opacity,
            overlay_renderer: overlay.renderer,
            overlay_fps_color: overlay.fps_color,
            startup,
            manual_step_fps,
            manual_min_fps,
            manual_max_fps,
            input_enabled: keys.input_enabled,
            hotkey_auto: hotkey_from_string(keys.hotkey_auto),
            hotkey_up: hotkey_from_string(keys.hotkey_up),
            hotkey_down: hotkey_from_string(keys.hotkey_down),
            hotkey_toggle: hotkey_from_string(keys.hotkey_toggle),
            cpu_temp_path: metrics.cpu_temp_path.clone(),
            gpu_vrm_temp_path: metrics.gpu_vrm_temp_path.clone(),
            cpu_vrm_temp_path: metrics.cpu_vrm_temp_path.clone(),
            gpu_edge_temp_path: metrics.gpu_edge_temp_path.clone(),
            gpu_power_path: metrics.gpu_power_path.clone(),
            cpu_power_path: metrics.cpu_power_path.clone(),
            ppt_power_path: metrics.ppt_power_path.clone(),
            fan_rpm_path: metrics.fan_rpm_path.clone(),
            sensor_settings: metrics,
        }
    }
}

fn positive_u32_or_default(value: u32, name: &str, default: u32) -> u32 {
    if value > 0 {
        value
    } else {
        diagnostics::error(format!(
            "Invalid [overlay].{name}: expected a value greater than 0 px; using {default} px"
        ));
        default
    }
}

#[cfg(test)]
mod scale_threshold_tests {
    use super::*;

    #[test]
    fn zero_scale_threshold_uses_the_default() {
        assert_eq!(
            positive_u32_or_default(
                0,
                "scale_2x_min_width_px",
                settings::OverlaySettings::default().scale_2x_min_width_px
            ),
            3840
        );
    }

    #[test]
    fn overlay_startup_accepts_three_modes_and_defaults_to_full() {
        assert_eq!(settings::Settings::default().overlay.startup, 2);
        for value in 0..=2 {
            assert_eq!(positive_u8(value, "startup", 0..=2, 2), value);
        }
        assert_eq!(positive_u8(3, "startup", 0..=2, 2), 2);
    }
}

/// Validates each GPU threshold as an independent percentage.
fn validated_gpu_thresholds(increase: f32, decrease: f32) -> (f32, f32) {
    if increase.is_finite()
        && decrease.is_finite()
        && (0.0..=100.0).contains(&increase)
        && (0.0..=100.0).contains(&decrease)
    {
        (increase, decrease)
    } else {
        diagnostics::error(format!(
            "Invalid [auto-mode.algorithms.gpu_threshold] GPU thresholds: expected gpu_increase_limit and gpu_decrease_limit to be finite percentages from 0 to 100; using {DEFAULT_GPU_INCREASE_LIMIT} and {DEFAULT_GPU_DECREASE_LIMIT}"
        ));
        (DEFAULT_GPU_INCREASE_LIMIT, DEFAULT_GPU_DECREASE_LIMIT)
    }
}

/// Parses the frame cap, treating zero as disabled.
fn parse_target_fps(value: Option<f32>) -> Result<Option<f32>, &'static str> {
    match value {
        Some(0.0) => Ok(None),
        Some(fps) => {
            if !fps.is_finite() || fps < 0.0 || fps > MAX_TARGET_FPS {
                return Err("target_fps must be 0 or between 1.0 and 240.0");
            }
            Ok(Some(fps))
        }
        None => Ok(None),
    }
}

/// Validates the configured sample count and returns its built-in default on error.
fn positive_window_size(value: usize) -> usize {
    if (1..=4096).contains(&value) {
        value
    } else {
        diagnostics::error(
            "Invalid [framecap].window_size: expected a value from 1 to 4096; using 60",
        );
        60
    }
}

/// Returns a duration within the supplied millisecond range.
pub fn bounded_milliseconds(
    value: Option<u64>,
    name: &str,
    minimum: u64,
    maximum: u64,
    default: u64,
) -> u64 {
    if let Some(value) = value {
        if (minimum..=maximum).contains(&value) {
            value
        } else {
            diagnostics::error(format!(
                "Invalid [framecap].{name}: expected {minimum}-{maximum} ms; using {default} ms"
            ));
            default
        }
    } else {
        default
    }
}

/// Rejects a zero unsigned integer.
pub fn positive_u64(value: u64) -> Result<u64, &'static str> {
    if value == 0 {
        return Err("value must be greater than 0");
    }
    Ok(value)
}

/// Rejects a zero millisecond duration.
pub fn positive_milliseconds(value: u64) -> Result<u64, &'static str> {
    if value == 0 {
        return Err("value must be greater than 0 ms");
    }
    Ok(value)
}

/// Validates an unsigned setting against an inclusive range.
pub fn positive_u8(value: u8, name: &str, range: std::ops::RangeInclusive<u8>, default: u8) -> u8 {
    if range.contains(&value) {
        value
    } else {
        diagnostics::error(format!(
            "Invalid [overlay].{name}: expected a value from {} to {}; using {}",
            range.start(),
            range.end(),
            default
        ));
        default
    }
}

/// Validates a finite float within an inclusive range.
pub fn bounded_finite(value: f32, name: &str, section: &str, minimum: f32, maximum: f32) -> f32 {
    if value.is_finite() && (minimum..=maximum).contains(&value) {
        value
    } else {
        diagnostics::error(format!(
            "Invalid [{}].{name}: expected a finite value between {} and {}; using {}",
            section, minimum, maximum, minimum
        ));
        minimum
    }
}

fn validated_auto_algorithms(algorithms: Vec<AutoAlgorithmSettings>) -> Vec<AutoAlgorithmConfig> {
    let defaults = settings::Settings::default().auto_mode.algorithms;
    let mut validated = Vec::new();
    for algorithm in algorithms {
        let Some(kind) = AutoAlgorithmKind::parse(&algorithm.name) else {
            diagnostics::error(format!(
                "Unknown [auto-mode.algorithms.{}] algorithm; ignoring it",
                algorithm.name
            ));
            continue;
        };
        let default = defaults
            .iter()
            .find(|default| default.name == algorithm.name)
            .expect("built-in auto algorithm defaults must include every supported algorithm");
        let valid_pair = match (
            AutoMetric::parse(&algorithm.metric),
            AutoTarget::from_settings(&algorithm.target),
        ) {
            (Some(AutoMetric::Fps), Some(AutoTarget::Low10Fps))
                if kind == AutoAlgorithmKind::GpuThreshold =>
            {
                Some((AutoMetric::Fps, AutoTarget::Low10Fps))
            }
            (Some(AutoMetric::GpuVrmTempC), Some(AutoTarget::Value(value)))
                if kind == AutoAlgorithmKind::VrmTemp =>
            {
                (value > 0.0).then_some((AutoMetric::GpuVrmTempC, AutoTarget::Value(value)))
            }
            (Some(AutoMetric::FanRpm), Some(AutoTarget::Value(value)))
                if kind == AutoAlgorithmKind::FanRpm =>
            {
                (value > 0.0).then_some((AutoMetric::FanRpm, AutoTarget::Value(value)))
            }
            _ => None,
        };
        let (metric, target) = if let Some(pair) = valid_pair {
            pair
        } else {
            diagnostics::error(format!(
                "Invalid metric/target for [auto-mode.algorithms.{}]; using its built-in metric and target",
                algorithm.name
            ));
            (
                AutoMetric::parse(&default.metric).expect("built-in metric must be valid"),
                AutoTarget::from_settings(&default.target).expect("built-in target must be valid"),
            )
        };
        let section = format!("auto-mode.algorithms.{}", algorithm.name);
        let hysteresis_percent = if algorithm.hysteresis_percent.is_finite()
            && (0.0..=100.0).contains(&algorithm.hysteresis_percent)
        {
            algorithm.hysteresis_percent
        } else {
            diagnostics::error(format!(
                "Invalid [{section}].hysteresis_percent: expected 0-100%; using {}%",
                default.hysteresis_percent
            ));
            default.hysteresis_percent
        };
        let increase_interval = validated_algorithm_interval(
            algorithm.auto_increase_interval_ms,
            "auto_increase_interval_ms",
            &section,
            default.auto_increase_interval_ms,
        );
        let decrease_interval = validated_algorithm_interval(
            algorithm.auto_decrease_interval_ms,
            "auto_decrease_interval_ms",
            &section,
            default.auto_decrease_interval_ms,
        );
        let increase_step_fps = positive_finite_or_default(
            algorithm.auto_increase_step_fps,
            "auto_increase_step_fps",
            &section,
            default.auto_increase_step_fps,
        );
        let decrease_step_fps = positive_finite_or_default(
            algorithm.auto_decrease_step_fps,
            "auto_decrease_step_fps",
            &section,
            default.auto_decrease_step_fps,
        );
        let (min_fps, max_fps) =
            validated_fps_bounds(algorithm.min_fps, algorithm.max_fps, &section);
        let measurement_expiry = validated_algorithm_interval(
            algorithm.measurement_expiry_ms,
            "measurement_expiry_ms",
            &section,
            default.measurement_expiry_ms,
        );
        validated.push(AutoAlgorithmConfig {
            name: algorithm.name,
            metric,
            target,
            hysteresis_percent,
            increase_interval,
            decrease_interval,
            increase_step_fps,
            decrease_step_fps,
            min_fps,
            max_fps,
            measurement_expiry,
        });
    }

    if validated.is_empty() {
        diagnostics::error("No usable automatic algorithms; using built-in algorithms");
        validated_auto_algorithms(defaults)
    } else {
        validated
    }
}

fn validated_algorithm_interval(value: u64, name: &str, section: &str, default: u64) -> Duration {
    match positive_milliseconds(value) {
        Ok(milliseconds) => Duration::from_millis(milliseconds),
        Err(reason) => {
            diagnostics::error(format!(
                "Invalid [{section}].{name}: {reason}; using {default} ms"
            ));
            Duration::from_millis(default)
        }
    }
}

/// Validates a finite range and returns the explicit default when invalid.
fn bounded_finite_or_default(
    value: f32,
    name: &str,
    section: &str,
    minimum: f32,
    maximum: f32,
    default: f32,
) -> f32 {
    if value.is_finite() && (minimum..=maximum).contains(&value) {
        value
    } else {
        diagnostics::error(format!(
            "Invalid [{section}].{name}: expected a finite value between {minimum} and {maximum}; using {default}"
        ));
        default
    }
}

/// Validates a positive finite float and returns the default when invalid.
fn positive_finite_or_default(value: f32, name: &str, section: &str, default: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        diagnostics::error(format!(
            "Invalid [{section}].{name}: expected a finite value greater than 0; using {default}"
        ));
        default
    }
}

/// Validates ordered FPS bounds against the supported target range.
fn validated_fps_bounds(minimum: f32, maximum: f32, section: &str) -> (f32, f32) {
    if minimum.is_finite()
        && maximum.is_finite()
        && (MIN_TARGET_FPS..=MAX_TARGET_FPS).contains(&minimum)
        && (MIN_TARGET_FPS..=MAX_TARGET_FPS).contains(&maximum)
        && minimum <= maximum
    {
        (minimum, maximum)
    } else {
        diagnostics::error(format!(
            "Invalid [{section}] FPS bounds: expected min_fps <= max_fps in the range {MIN_TARGET_FPS}-{MAX_TARGET_FPS}; using {MIN_TARGET_FPS}-{MAX_TARGET_FPS}"
        ));
        (MIN_TARGET_FPS, MAX_TARGET_FPS)
    }
}

/// Validates a finite non-negative float and substitutes its default if needed.
pub fn non_negative_finite(value: f32, name: &str, section: &str, default: f32) -> f32 {
    if value.is_finite() && value >= 0.0 {
        value
    } else {
        diagnostics::error(format!(
            "Invalid [{}].{name}: expected a non-negative finite number; using {}",
            section, default
        ));
        default
    }
}

/// Converts a configured key name to a function-key enum.
fn hotkey_from_string(value: String) -> Hotkey {
    match value.as_str() {
        "F9" => Hotkey::F9,
        "F10" => Hotkey::F10,
        "F11" => Hotkey::F11,
        "F12" => Hotkey::F12,
        _ => Hotkey::F9,
    }
}

#[cfg(test)]
mod tests {
    use super::{validated_gpu_thresholds, AutoAlgorithmKind, AutoMetric, AutoTarget, Config};
    use std::time::Duration;

    #[test]
    /// Checks the predefined algorithm names and rejects removed adjustment rules.
    fn predefined_auto_algorithm_names_parse() {
        assert_eq!(
            AutoAlgorithmKind::parse("gpu_threshold"),
            Some(AutoAlgorithmKind::GpuThreshold)
        );
        assert_eq!(
            AutoAlgorithmKind::parse("vrm_temp"),
            Some(AutoAlgorithmKind::VrmTemp)
        );
        assert_eq!(
            AutoAlgorithmKind::parse("fan_rpm"),
            Some(AutoAlgorithmKind::FanRpm)
        );
        assert_eq!(AutoAlgorithmKind::parse("unknown"), None);
    }

    #[test]
    fn shipped_algorithms_keep_order_and_independent_settings() {
        let config = Config::load();
        assert_eq!(config.auto_algorithms.len(), 3);
        assert_eq!(config.auto_algorithms[0].name, "gpu_threshold");
        assert_eq!(config.auto_algorithms[0].metric, AutoMetric::Fps);
        assert_eq!(config.auto_algorithms[0].target, AutoTarget::Low10Fps);
        assert_eq!(
            config.auto_algorithms[0].increase_interval,
            Duration::from_millis(4000)
        );
        assert_eq!(config.gpu_increase_limit, 98.0);
        assert_eq!(config.gpu_decrease_limit, 90.0);
        assert_eq!(config.frame_jitter_limit_ms, 2.0);
        assert_eq!(config.auto_algorithms[1].name, "vrm_temp");
        assert_eq!(config.auto_algorithms[1].metric, AutoMetric::GpuVrmTempC);
        assert_eq!(config.auto_algorithms[1].target, AutoTarget::Value(100.0));
        assert_eq!(
            config.auto_algorithms[1].increase_interval,
            Duration::from_millis(10000)
        );
        assert_eq!(config.auto_algorithms[2].name, "fan_rpm");
        assert_eq!(config.auto_algorithms[2].metric, AutoMetric::FanRpm);
        assert_eq!(config.auto_algorithms[2].target, AutoTarget::Value(2000.0));
        assert_eq!(
            config.auto_algorithms[2].increase_interval,
            Duration::from_millis(5000)
        );
        assert_eq!(
            crate::layer::settings::Settings::default()
                .auto_mode
                .algorithms[2]
                .auto_increase_interval_ms,
            10000
        );
    }

    #[test]
    fn gpu_threshold_limits_load_from_the_algorithm_section() {
        let config = Config::load();
        assert_eq!(config.gpu_increase_limit, 98.0);
        assert_eq!(config.gpu_decrease_limit, 90.0);
        assert_eq!(config.frame_jitter_limit_ms, 2.0);
    }

    #[test]
    /// Checks that GPU thresholds are independently bounded percentages.
    fn gpu_thresholds_are_independent_bounded_percentages() {
        assert_eq!(validated_gpu_thresholds(85.0, 97.0), (85.0, 97.0));
        assert_eq!(validated_gpu_thresholds(98.0, 90.0), (98.0, 90.0));
        assert_eq!(validated_gpu_thresholds(101.0, 90.0), (98.0, 90.0));
        assert_eq!(validated_gpu_thresholds(98.0, -1.0), (98.0, 90.0));
    }

    #[test]
    /// Checks the configured jitter limit range and its built-in default.
    fn frame_jitter_limit_is_bounded_in_milliseconds() {
        use super::{bounded_finite_or_default, DEFAULT_FRAME_JITTER_LIMIT_MS};

        let validate = |value| {
            bounded_finite_or_default(
                value,
                "frame_jitter_limit",
                "auto-mode.algorithms.gpu_threshold",
                0.0,
                500.0,
                2.0,
            )
        };
        assert_eq!(validate(0.0), 0.0);
        assert_eq!(validate(500.0), 500.0);
        assert_eq!(validate(-0.1), DEFAULT_FRAME_JITTER_LIMIT_MS);
        assert_eq!(validate(f32::NAN), DEFAULT_FRAME_JITTER_LIMIT_MS);
    }
}
