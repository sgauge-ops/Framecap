use serde::Deserialize;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct FramecapSettings {
    pub target_fps: f32,
    pub window_size: usize,
    pub gpu_poll_rate_ms: u64,
}

impl Default for FramecapSettings {
    /// Returns the built-in frame cap, sample window, and GPU polling defaults.
    fn default() -> Self {
        Self {
            target_fps: 60.0,
            window_size: 60,
            gpu_poll_rate_ms: 100,
        }
    }
}

#[derive(Clone, Debug)]
pub struct KeyBindingsSettings {
    pub input_enabled: bool,
    pub hotkey_auto: String,
    pub hotkey_up: String,
    pub hotkey_down: String,
    pub hotkey_toggle: String,
}

impl Default for KeyBindingsSettings {
    /// Returns the built-in function-key mapping.
    fn default() -> Self {
        Self {
            input_enabled: true,
            hotkey_auto: "F9".to_owned(),
            hotkey_up: "F11".to_owned(),
            hotkey_down: "F10".to_owned(),
            hotkey_toggle: "F12".to_owned(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct OverlaySettings {
    pub renderer: OverlayRenderer,
    pub refresh_rate: u64,
    pub gpu_smoothing_window_ms: u64,
    pub scale_2x_min_width_px: u32,
    pub background_opacity: f32,
    pub fps_color: [u8; 3],
    pub startup: u8,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OverlayRenderer {
    #[default]
    Ascii,
    Modern,
}

impl Default for OverlaySettings {
    /// Returns the built-in overlay refresh, smoothing, and display defaults.
    fn default() -> Self {
        Self {
            renderer: OverlayRenderer::default(),
            refresh_rate: 10,
            gpu_smoothing_window_ms: 1000,
            scale_2x_min_width_px: 3840,
            background_opacity: 0.25,
            fps_color: [255, 255, 0],
            startup: 2,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AutoModeSettings {
    pub algorithm_order: Vec<String>,
    pub algorithms: Vec<AutoAlgorithmSettings>,
}

#[derive(Clone, Debug)]
pub struct GpuThresholdSettings {
    pub gpu_increase_limit: f32,
    pub gpu_decrease_limit: f32,
    pub frame_jitter_limit: f32,
}

impl Default for GpuThresholdSettings {
    fn default() -> Self {
        Self {
            gpu_increase_limit: 98.0,
            gpu_decrease_limit: 90.0,
            frame_jitter_limit: 2.0,
        }
    }
}

#[derive(Clone, Debug)]
pub enum AutoTargetSettings {
    Metric(String),
    Value(f32),
}

#[derive(Clone, Debug)]
pub struct AutoAlgorithmSettings {
    pub name: String,
    pub metric: String,
    pub target: AutoTargetSettings,
    pub hysteresis_percent: f32,
    pub auto_increase_interval_ms: u64,
    pub auto_decrease_interval_ms: u64,
    pub auto_increase_step_fps: f32,
    pub auto_decrease_step_fps: f32,
    pub min_fps: f32,
    pub max_fps: f32,
    pub measurement_expiry_ms: u64,
    pub gpu_threshold: Option<GpuThresholdSettings>,
}

impl Default for AutoModeSettings {
    /// Returns the built-in automatic-regulation defaults.
    fn default() -> Self {
        Self {
            algorithm_order: vec![
                "gpu_threshold".to_owned(),
                "vrm_temp".to_owned(),
                "fan_rpm".to_owned(),
            ],
            algorithms: vec![
                AutoAlgorithmSettings {
                    name: "gpu_threshold".to_owned(),
                    metric: "fps".to_owned(),
                    target: AutoTargetSettings::Metric("low_10_fps".to_owned()),
                    hysteresis_percent: 3.0,
                    auto_increase_interval_ms: 5000,
                    auto_decrease_interval_ms: 5000,
                    auto_increase_step_fps: 5.0,
                    auto_decrease_step_fps: 5.0,
                    min_fps: 1.0,
                    max_fps: 240.0,
                    measurement_expiry_ms: 1000,
                    gpu_threshold: Some(GpuThresholdSettings::default()),
                },
                AutoAlgorithmSettings {
                    name: "vrm_temp".to_owned(),
                    metric: "gpu_vrm_temp_c".to_owned(),
                    target: AutoTargetSettings::Value(100.0),
                    hysteresis_percent: 5.0,
                    auto_increase_interval_ms: 10000,
                    auto_decrease_interval_ms: 5000,
                    auto_increase_step_fps: 5.0,
                    auto_decrease_step_fps: 5.0,
                    min_fps: 1.0,
                    max_fps: 240.0,
                    measurement_expiry_ms: 1000,
                    gpu_threshold: None,
                },
                AutoAlgorithmSettings {
                    name: "fan_rpm".to_owned(),
                    metric: "fan_rpm".to_owned(),
                    target: AutoTargetSettings::Value(2000.0),
                    hysteresis_percent: 5.0,
                    auto_increase_interval_ms: 10000,
                    auto_decrease_interval_ms: 5000,
                    auto_increase_step_fps: 5.0,
                    auto_decrease_step_fps: 5.0,
                    min_fps: 1.0,
                    max_fps: 240.0,
                    measurement_expiry_ms: 1000,
                    gpu_threshold: None,
                },
            ],
        }
    }
}

#[derive(Clone, Debug)]
pub struct ManualModeSettings {
    pub manual_step_fps: f32,
    pub manual_min_fps: f32,
    pub manual_max_fps: f32,
}

impl Default for ManualModeSettings {
    /// Returns the built-in manual-regulation defaults.
    fn default() -> Self {
        Self {
            manual_step_fps: 5.0,
            manual_min_fps: 1.0,
            manual_max_fps: 240.0,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct DiagnosticsSettings {
    pub log_path: Option<PathBuf>,
    pub append_log: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SensorSelector {
    pub device: String,
    pub label: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MetricsSettings {
    pub cpu_temp_path: Option<PathBuf>,
    pub gpu_vrm_temp_path: Option<PathBuf>,
    pub cpu_vrm_temp_path: Option<PathBuf>,
    pub gpu_edge_temp_path: Option<PathBuf>,
    pub gpu_power_path: Option<PathBuf>,
    pub cpu_power_path: Option<PathBuf>,
    pub ppt_power_path: Option<PathBuf>,
    pub fan_rpm_path: Option<PathBuf>,
    pub cpu_temp: Option<SensorSelector>,
    pub gpu_vrm_temp: Option<SensorSelector>,
    pub cpu_vrm_temp: Option<SensorSelector>,
    pub gpu_edge_temp: Option<SensorSelector>,
    pub gpu_power: Option<SensorSelector>,
    pub cpu_power: Option<SensorSelector>,
    pub ppt_power: Option<SensorSelector>,
    pub fan_rpm: Option<SensorSelector>,
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub framecap: FramecapSettings,
    pub key_bindings: KeyBindingsSettings,
    pub overlay: OverlaySettings,
    pub auto_mode: AutoModeSettings,
    pub manual_mode: ManualModeSettings,
    pub diagnostics: DiagnosticsSettings,
    pub metrics: MetricsSettings,
}

impl Default for Settings {
    /// Returns defaults for every Framecap configuration section.
    fn default() -> Self {
        Self {
            framecap: FramecapSettings::default(),
            key_bindings: KeyBindingsSettings::default(),
            overlay: OverlaySettings::default(),
            auto_mode: AutoModeSettings::default(),
            manual_mode: ManualModeSettings::default(),
            diagnostics: DiagnosticsSettings::default(),
            metrics: MetricsSettings::default(),
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct RawSettings {
    framecap: RawFramecapSettings,
    #[serde(rename = "key-bindings")]
    key_bindings: RawKeyBindingsSettings,
    overlay: RawOverlaySettings,
    #[serde(rename = "auto-mode")]
    auto_mode: RawAutoModeSettings,
    #[serde(rename = "manual-mode")]
    manual_mode: RawManualModeSettings,
    diagnostics: RawDiagnosticsSettings,
    #[serde(rename = "sensors")]
    metrics: RawMetricsSettings,
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct RawFramecapSettings {
    target_fps: Option<f32>,
    window_size: Option<usize>,
    gpu_poll_rate_ms: Option<u64>,
    input_enabled: Option<bool>,
    hotkey_auto: Option<String>,
    hotkey_up: Option<String>,
    hotkey_down: Option<String>,
    hotkey_toggle: Option<String>,
    jitter_high: Option<f32>,
    jitter_low: Option<f32>,
    gpu_saturation: Option<f32>,
    gpu_smoothing_window_ms: Option<u64>,
    background_opacity: Option<f32>,
    startup: Option<u8>,
    manual_step_fps: Option<f32>,
    manual_min_fps: Option<f32>,
    manual_max_fps: Option<f32>,
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct RawKeyBindingsSettings {
    input_enabled: Option<bool>,
    hotkey_auto: Option<String>,
    hotkey_up: Option<String>,
    hotkey_down: Option<String>,
    hotkey_toggle: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct RawOverlaySettings {
    renderer: Option<OverlayRenderer>,
    refresh_rate: Option<u64>,
    gpu_smoothing_window_ms: Option<u64>,
    scale_2x_min_width_px: Option<u32>,
    background_opacity: Option<f32>,
    #[serde(deserialize_with = "deserialize_rgb")]
    fps_color: Option<[u8; 3]>,
    startup: Option<u8>,
}

fn deserialize_rgb<'de, D>(deserializer: D) -> Result<Option<[u8; 3]>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let channels = Option::<Vec<u8>>::deserialize(deserializer)?;
    channels
        .map(|channels| {
            channels
                .try_into()
                .map_err(|_| serde::de::Error::custom("expected exactly three RGB channels"))
        })
        .transpose()
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct RawAutoModeSettings {
    algorithm_order: Option<Vec<String>>,
    algorithms: Option<BTreeMap<String, RawAutoAlgorithmSettings>>,
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct RawAutoAlgorithmSettings {
    metric: Option<String>,
    target: Option<RawAutoTarget>,
    hysteresis_percent: Option<f32>,
    auto_increase_interval_ms: Option<u64>,
    auto_decrease_interval_ms: Option<u64>,
    auto_increase_step_fps: Option<f32>,
    auto_decrease_step_fps: Option<f32>,
    min_fps: Option<f32>,
    max_fps: Option<f32>,
    measurement_expiry_ms: Option<u64>,
    gpu_increase_limit: Option<f32>,
    gpu_decrease_limit: Option<f32>,
    frame_jitter_limit: Option<f32>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawAutoTarget {
    Metric(String),
    Value(f32),
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct RawManualModeSettings {
    #[serde(rename = "step_fps")]
    manual_step_fps: Option<f32>,
    #[serde(rename = "min_fps")]
    manual_min_fps: Option<f32>,
    #[serde(rename = "max_fps")]
    manual_max_fps: Option<f32>,
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct RawDiagnosticsSettings {
    log_path: Option<PathBuf>,
    append_log: Option<u8>,
}

type RawMetricsSettings = MetricsSettings;

impl Settings {
    /// Loads the first available TOML configuration and returns normalized settings.
    pub fn load() -> (Self, Option<PathBuf>, bool) {
        let candidates = config_candidates();
        let Some(path) = candidates.iter().find(|path| path.is_file()).cloned() else {
            return (Self::default(), candidates.into_iter().next(), false);
        };

        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) => {
                eprintln!(
                    "Framecap: unable to read configuration {}: {error}",
                    path.display()
                );
                return (Self::default(), Some(path), false);
            }
        };
        let raw: RawSettings = match toml::from_str(&content) {
            Ok(raw) => raw,
            Err(error) => {
                eprintln!(
                    "Framecap: invalid configuration {}: {error}; using built-in defaults",
                    path.display()
                );
                return (Self::default(), Some(path), false);
            }
        };

        let legacy = has_legacy_settings(&raw.framecap);
        if legacy {
            eprintln!(
                "Framecap: legacy [framecap] settings detected; migrate supported keys to their dedicated sections"
            );
        }

        let RawSettings {
            framecap,
            key_bindings,
            overlay,
            auto_mode,
            manual_mode,
            diagnostics,
            metrics,
        } = raw;
        let defaults = Self::default();
        let append_log = diagnostics_append_log(diagnostics.append_log);

        let settings = Self {
            framecap: FramecapSettings {
                target_fps: framecap.target_fps.unwrap_or(defaults.framecap.target_fps),
                window_size: framecap
                    .window_size
                    .unwrap_or(defaults.framecap.window_size),
                gpu_poll_rate_ms: framecap
                    .gpu_poll_rate_ms
                    .unwrap_or(defaults.framecap.gpu_poll_rate_ms),
            },
            key_bindings: KeyBindingsSettings {
                input_enabled: key_bindings
                    .input_enabled
                    .or(framecap.input_enabled)
                    .unwrap_or(defaults.key_bindings.input_enabled),
                hotkey_auto: key_bindings
                    .hotkey_auto
                    .or(framecap.hotkey_auto)
                    .unwrap_or(defaults.key_bindings.hotkey_auto),
                hotkey_up: key_bindings
                    .hotkey_up
                    .or(framecap.hotkey_up)
                    .unwrap_or(defaults.key_bindings.hotkey_up),
                hotkey_down: key_bindings
                    .hotkey_down
                    .or(framecap.hotkey_down)
                    .unwrap_or(defaults.key_bindings.hotkey_down),
                hotkey_toggle: key_bindings
                    .hotkey_toggle
                    .or(framecap.hotkey_toggle)
                    .unwrap_or(defaults.key_bindings.hotkey_toggle),
            },
            overlay: OverlaySettings {
                renderer: overlay.renderer.unwrap_or_default(),
                refresh_rate: overlay
                    .refresh_rate
                    .unwrap_or(defaults.overlay.refresh_rate),
                gpu_smoothing_window_ms: overlay
                    .gpu_smoothing_window_ms
                    .or(framecap.gpu_smoothing_window_ms)
                    .unwrap_or(defaults.overlay.gpu_smoothing_window_ms),
                scale_2x_min_width_px: overlay
                    .scale_2x_min_width_px
                    .unwrap_or(defaults.overlay.scale_2x_min_width_px),
                background_opacity: overlay
                    .background_opacity
                    .or(framecap.background_opacity)
                    .unwrap_or(defaults.overlay.background_opacity),
                fps_color: overlay.fps_color.unwrap_or(defaults.overlay.fps_color),
                startup: overlay
                    .startup
                    .or(framecap.startup)
                    .unwrap_or(defaults.overlay.startup),
            },
            auto_mode: AutoModeSettings {
                algorithm_order: auto_mode
                    .algorithm_order
                    .clone()
                    .unwrap_or_else(|| defaults.auto_mode.algorithm_order.clone()),
                algorithms: parse_auto_algorithms(
                    auto_mode.algorithms,
                    auto_mode
                        .algorithm_order
                        .unwrap_or(defaults.auto_mode.algorithm_order),
                    defaults.auto_mode.algorithms,
                ),
            },
            manual_mode: ManualModeSettings {
                manual_step_fps: manual_mode
                    .manual_step_fps
                    .or(framecap.manual_step_fps)
                    .unwrap_or(defaults.manual_mode.manual_step_fps),
                manual_min_fps: manual_mode
                    .manual_min_fps
                    .or(framecap.manual_min_fps)
                    .unwrap_or(defaults.manual_mode.manual_min_fps),
                manual_max_fps: manual_mode
                    .manual_max_fps
                    .or(framecap.manual_max_fps)
                    .unwrap_or(defaults.manual_mode.manual_max_fps),
            },
            diagnostics: DiagnosticsSettings {
                log_path: diagnostics.log_path,
                append_log,
            },
            metrics,
        };

        (settings, Some(path), legacy)
    }
}

/// Reports whether the legacy `[framecap]` section contains moved settings.
fn has_legacy_settings(framecap: &RawFramecapSettings) -> bool {
    framecap.jitter_high.is_some()
        || framecap.jitter_low.is_some()
        || framecap.gpu_saturation.is_some()
        || framecap.input_enabled.is_some()
        || framecap.hotkey_auto.is_some()
        || framecap.hotkey_up.is_some()
        || framecap.hotkey_down.is_some()
        || framecap.hotkey_toggle.is_some()
        || framecap.gpu_smoothing_window_ms.is_some()
        || framecap.background_opacity.is_some()
        || framecap.startup.is_some()
        || framecap.manual_step_fps.is_some()
        || framecap.manual_min_fps.is_some()
        || framecap.manual_max_fps.is_some()
}

fn parse_auto_algorithms(
    raw_algorithms: Option<BTreeMap<String, RawAutoAlgorithmSettings>>,
    order: Vec<String>,
    defaults: Vec<AutoAlgorithmSettings>,
) -> Vec<AutoAlgorithmSettings> {
    let Some(raw_algorithms) = raw_algorithms else {
        let mut seen = std::collections::HashSet::new();
        let ordered = order
            .into_iter()
            .filter(|name| seen.insert(name.clone()))
            .filter_map(|name| defaults.iter().find(|algorithm| algorithm.name == name))
            .cloned()
            .collect::<Vec<_>>();
        return if ordered.is_empty() {
            defaults
        } else {
            ordered
        };
    };
    if raw_algorithms.is_empty() {
        eprintln!("Framecap: [auto-mode].algorithms is empty; using built-in algorithms");
        return defaults;
    }

    parse_ordered_auto_algorithms(raw_algorithms, order, defaults)
}

fn parse_ordered_auto_algorithms(
    mut raw_algorithms: BTreeMap<String, RawAutoAlgorithmSettings>,
    order: Vec<String>,
    defaults: Vec<AutoAlgorithmSettings>,
) -> Vec<AutoAlgorithmSettings> {
    let mut algorithms = Vec::with_capacity(order.len());
    let mut seen = std::collections::HashSet::new();
    for name in order {
        if !seen.insert(name.clone()) {
            eprintln!(
                "Framecap: duplicate auto-mode algorithm {name:?} in algorithm_order; ignoring it"
            );
            continue;
        }
        let Some(raw) = raw_algorithms.remove(&name) else {
            eprintln!("Framecap: [auto-mode.algorithms.{name}] is missing; ignoring it");
            continue;
        };
        let Some(default) = defaults.iter().find(|algorithm| algorithm.name == name) else {
            eprintln!("Framecap: unknown auto-mode algorithm {name:?}; ignoring it");
            continue;
        };
        let target = raw.target.map(|target| match target {
            RawAutoTarget::Metric(metric) => AutoTargetSettings::Metric(metric),
            RawAutoTarget::Value(value) => AutoTargetSettings::Value(value),
        });
        let is_gpu_threshold = name == "gpu_threshold";
        let gpu_threshold = if is_gpu_threshold {
            let threshold_defaults = default
                .gpu_threshold
                .as_ref()
                .expect("gpu_threshold defaults must include its gates");
            Some(GpuThresholdSettings {
                gpu_increase_limit: raw
                    .gpu_increase_limit
                    .unwrap_or(threshold_defaults.gpu_increase_limit),
                gpu_decrease_limit: raw
                    .gpu_decrease_limit
                    .unwrap_or(threshold_defaults.gpu_decrease_limit),
                frame_jitter_limit: raw
                    .frame_jitter_limit
                    .unwrap_or(threshold_defaults.frame_jitter_limit),
            })
        } else {
            if raw.gpu_increase_limit.is_some()
                || raw.gpu_decrease_limit.is_some()
                || raw.frame_jitter_limit.is_some()
            {
                eprintln!(
                    "Framecap: GPU threshold settings are only valid in [auto-mode.algorithms.gpu_threshold]; ignoring them for {name}"
                );
            }
            None
        };
        algorithms.push(AutoAlgorithmSettings {
            name,
            metric: raw.metric.unwrap_or_else(|| default.metric.clone()),
            target: target.unwrap_or_else(|| default.target.clone()),
            hysteresis_percent: raw.hysteresis_percent.unwrap_or(default.hysteresis_percent),
            auto_increase_interval_ms: raw
                .auto_increase_interval_ms
                .unwrap_or(default.auto_increase_interval_ms),
            auto_decrease_interval_ms: raw
                .auto_decrease_interval_ms
                .unwrap_or(default.auto_decrease_interval_ms),
            auto_increase_step_fps: raw
                .auto_increase_step_fps
                .unwrap_or(default.auto_increase_step_fps),
            auto_decrease_step_fps: raw
                .auto_decrease_step_fps
                .unwrap_or(default.auto_decrease_step_fps),
            min_fps: raw.min_fps.unwrap_or(default.min_fps),
            max_fps: raw.max_fps.unwrap_or(default.max_fps),
            measurement_expiry_ms: raw
                .measurement_expiry_ms
                .unwrap_or(default.measurement_expiry_ms),
            gpu_threshold,
        });
    }
    for name in raw_algorithms.keys() {
        eprintln!(
            "Framecap: [auto-mode.algorithms.{name}] is not listed in algorithm_order; ignoring it"
        );
    }

    if algorithms.is_empty() {
        eprintln!("Framecap: no valid auto-mode algorithms; using built-in algorithms");
        defaults
    } else {
        algorithms
    }
}

/// Lists configuration paths in the precedence order used by the loader.
fn config_candidates() -> Vec<PathBuf> {
    let mut candidates = vec![PathBuf::from("framecap.toml")];
    if let Some(xdg_config) = env::var_os("XDG_CONFIG_HOME") {
        candidates.push(PathBuf::from(xdg_config).join("framecap.toml"));
    }
    if let Some(home) = env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join(".config/framecap.toml"));
    }
    candidates
}

/// Resolves the log-retention option to an append policy.
fn diagnostics_append_log(append_log: Option<u8>) -> bool {
    match append_log.unwrap_or(0) {
        0 => false,
        1 => true,
        value => {
            eprintln!("Framecap: invalid [diagnostics].append_log={value}; using 0");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_renderer_is_opt_in_and_rejects_unknown_values() {
        assert_eq!(Settings::default().overlay.renderer, OverlayRenderer::Ascii);
        for (value, expected) in [
            ("ascii", OverlayRenderer::Ascii),
            ("modern", OverlayRenderer::Modern),
        ] {
            let settings: RawSettings =
                toml::from_str(&format!("[overlay]\nrenderer = \"{value}\"")).unwrap();
            assert_eq!(settings.overlay.renderer, Some(expected));
        }
        for value in ["classic", "dashboard", "other"] {
            assert!(
                toml::from_str::<RawSettings>(&format!("[overlay]\nrenderer = \"{value}\""))
                    .is_err()
            );
        }
    }

    #[test]
    fn overlay_fps_color_defaults_to_yellow_and_accepts_rgb_bytes() {
        assert_eq!(Settings::default().overlay.fps_color, [255, 255, 0]);
        let settings: RawSettings = toml::from_str("[overlay]\nfps_color = [0, 128, 255]").unwrap();
        assert_eq!(settings.overlay.fps_color, Some([0, 128, 255]));
        for invalid in [
            "[-1, 0, 0]",
            "[256, 0, 0]",
            "[0, 0]",
            "[0, 0, 0, 0]",
            "[0.5, 0, 0]",
            "\"yellow\"",
        ] {
            assert!(
                toml::from_str::<RawSettings>(&format!("[overlay]\nfps_color = {invalid}"))
                    .is_err(),
                "accepted invalid color: {invalid}"
            );
        }
    }

    #[test]
    /// Checks the built-in and configured 2x overlay width threshold.
    fn overlay_scale_threshold_defaults_to_3840_and_is_configurable() {
        assert_eq!(Settings::default().overlay.scale_2x_min_width_px, 3840);

        let settings: RawSettings =
            toml::from_str("[overlay]\nscale_2x_min_width_px = 2560").unwrap();
        assert_eq!(settings.overlay.scale_2x_min_width_px, Some(2560));
    }

    #[test]
    /// Checks that configured algorithms share one schema while retaining individual values.
    fn auto_algorithms_parse_shared_parameters() {
        let settings: RawSettings = toml::from_str(
            r#"
                [auto-mode]
                algorithm_order = ["vrm_temp", "gpu_threshold", "fan_rpm"]

                [auto-mode.algorithms.gpu_threshold]
                metric = "fps"
                target = "low_10_fps"
                hysteresis_percent = 4.0
                auto_increase_interval_ms = 3000
                auto_decrease_interval_ms = 2000
                auto_increase_step_fps = 5.0
                auto_decrease_step_fps = 5.0
                min_fps = 40.0
                max_fps = 120.0
                measurement_expiry_ms = 1000

                [auto-mode.algorithms.vrm_temp]
                metric = "gpu_vrm_temp_c"
                target = 100.0
                hysteresis_percent = 5.0
                auto_increase_interval_ms = 10000
                auto_decrease_interval_ms = 5000
                auto_increase_step_fps = 5.0
                auto_decrease_step_fps = 5.0
                min_fps = 40.0
                max_fps = 120.0
                measurement_expiry_ms = 2000

                [auto-mode.algorithms.fan_rpm]
                metric = "fan_rpm"
                target = 2000.0
                hysteresis_percent = 5.0
                auto_increase_interval_ms = 10000
                auto_decrease_interval_ms = 5000
                auto_increase_step_fps = 5.0
                auto_decrease_step_fps = 5.0
                min_fps = 40.0
                max_fps = 120.0
                measurement_expiry_ms = 2000
                "#,
        )
        .unwrap();
        let algorithms = parse_auto_algorithms(
            settings.auto_mode.algorithms,
            settings.auto_mode.algorithm_order.unwrap(),
            Settings::default().auto_mode.algorithms,
        );
        assert_eq!(algorithms.len(), 3);
        assert_eq!(algorithms[0].name, "vrm_temp");
        assert_eq!(algorithms[0].metric, "gpu_vrm_temp_c");
        assert_eq!(algorithms[1].name, "gpu_threshold");
        assert_eq!(algorithms[1].metric, "fps");
        assert_eq!(algorithms[2].name, "fan_rpm");
        assert_eq!(algorithms[2].metric, "fan_rpm");
    }

    #[test]
    /// Checks parsing of configurable GPU threshold percentages.
    fn gpu_threshold_settings_belong_to_the_gpu_threshold_algorithm() {
        let defaults = Settings::default();
        let defaults = defaults.auto_mode.algorithms[0]
            .gpu_threshold
            .as_ref()
            .unwrap();
        assert_eq!(defaults.gpu_increase_limit, 98.0);
        assert_eq!(defaults.gpu_decrease_limit, 90.0);
        assert_eq!(defaults.frame_jitter_limit, 2.0);

        let settings: RawSettings = toml::from_str(
            "[auto-mode.algorithms.gpu_threshold]\ngpu_increase_limit = 85\ngpu_decrease_limit = 97\nframe_jitter_limit = 3.5\n[auto-mode.algorithms.vrm_temp]\nmetric = \"gpu_vrm_temp_c\"\ntarget = 100.0",
        )
        .unwrap();
        let algorithms = parse_auto_algorithms(
            settings.auto_mode.algorithms,
            vec!["gpu_threshold".to_owned(), "vrm_temp".to_owned()],
            Settings::default().auto_mode.algorithms,
        );
        let gpu_threshold = algorithms[0].gpu_threshold.as_ref().unwrap();
        assert_eq!(gpu_threshold.gpu_increase_limit, 85.0);
        assert_eq!(gpu_threshold.gpu_decrease_limit, 97.0);
        assert_eq!(gpu_threshold.frame_jitter_limit, 3.5);
        assert!(algorithms[1].gpu_threshold.is_none());
    }

    #[test]
    /// Rejects GPU-threshold settings at the shared auto-mode level.
    fn gpu_threshold_settings_are_not_shared_auto_mode_settings() {
        assert!(toml::from_str::<RawSettings>("[auto-mode]\ngpu_increase_limit = 85").is_err());
    }

    #[test]
    /// Checks that hwmon paths can be configured independently.
    fn hwmon_sensor_paths_are_configurable() {
        let settings: RawSettings = toml::from_str(
            "[sensors]\ngpu_vrm_temp_path = \"/sys/class/hwmon/hwmon4/temp2_input\"\ncpu_vrm_temp_path = \"/sys/class/hwmon/hwmon4/temp1_input\"\ngpu_edge_temp_path = \"/sys/class/hwmon/hwmon1/temp1_input\"\ngpu_power_path = \"/sys/class/hwmon/hwmon4/power2\"\ncpu_power_path = \"/sys/class/hwmon/hwmon4/power1\"\nppt_power_path = \"/sys/class/hwmon/hwmon1/power1\"\nfan_rpm_path = \"/sys/class/hwmon/hwmon3/fan1\"",
        )
        .unwrap();

        assert_eq!(
            settings.metrics.gpu_vrm_temp_path,
            Some(PathBuf::from("/sys/class/hwmon/hwmon4/temp2_input"))
        );
        assert_eq!(
            settings.metrics.ppt_power_path,
            Some(PathBuf::from("/sys/class/hwmon/hwmon1/power1"))
        );
        assert_eq!(
            settings.metrics.gpu_power_path,
            Some(PathBuf::from("/sys/class/hwmon/hwmon4/power2"))
        );
        assert_eq!(
            settings.metrics.fan_rpm_path,
            Some(PathBuf::from("/sys/class/hwmon/hwmon3/fan1"))
        );
    }

    #[test]
    fn hwmon_sensor_selectors_are_configurable() {
        let settings: RawSettings = toml::from_str(
            r#"[sensors]
            fan_rpm = { device = "nct6686", label = "Pump Fan" }
            ppt_power = { device = "amdgpu", label = "PPT" }
            "#,
        )
        .unwrap();
        assert_eq!(
            settings.metrics.fan_rpm,
            Some(SensorSelector {
                device: "nct6686".into(),
                label: "Pump Fan".into(),
            })
        );
        for invalid in [
            r#"fan_rpm = { device = "nct6686" }"#,
            r#"fan_rpm = { label = "Pump Fan" }"#,
            r#"fan_rpm = { device = "nct6686", label = "Pump Fan", index = 2 }"#,
        ] {
            assert!(toml::from_str::<RawSettings>(&format!("[sensors]\n{invalid}")).is_err());
        }
    }

    #[test]
    /// Checks that the shipped example parses with its active GPU and jitter thresholds.
    fn shipped_configuration_parses_gpu_and_jitter_thresholds() {
        let settings: RawSettings = toml::from_str(include_str!("../../framecap.toml")).unwrap();
        assert_eq!(settings.overlay.renderer, Some(OverlayRenderer::Modern));
        let gpu_threshold = settings
            .auto_mode
            .algorithms
            .unwrap()
            .remove("gpu_threshold")
            .unwrap();
        assert_eq!(gpu_threshold.gpu_increase_limit, Some(98.0));
        assert_eq!(gpu_threshold.gpu_decrease_limit, Some(90.0));
        assert_eq!(gpu_threshold.frame_jitter_limit, Some(2.0));
    }

    #[test]
    /// Checks that `append_log` accepts integer zero and one.
    fn append_log_uses_integer_zero_or_one() {
        assert!(!Settings::default().diagnostics.append_log);

        let settings: RawSettings = toml::from_str("[diagnostics]\nappend_log = 0").unwrap();
        assert_eq!(settings.diagnostics.append_log, Some(0));
        assert!(!diagnostics_append_log(settings.diagnostics.append_log));
        let settings: RawSettings = toml::from_str("[diagnostics]\nappend_log = 1").unwrap();
        assert_eq!(settings.diagnostics.append_log, Some(1));
        assert!(diagnostics_append_log(settings.diagnostics.append_log));
    }

    #[test]
    /// Checks that the removed `keep_log` option is rejected.
    fn keep_log_is_not_a_valid_diagnostics_option() {
        assert!(toml::from_str::<RawSettings>("[diagnostics]\nkeep_log = 1").is_err());
    }
}
