use crate::layer::diagnostics;
pub use crate::layer::lib::TelemetryStatus;
use crate::layer::lib::{GpuMeasurement, HardwareMetrics};
use crate::layer::settings::{MetricsSettings, SensorSelector};
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

pub(crate) struct GpuUsageSmoother {
    window: Duration,
    samples: VecDeque<(Instant, f32)>,
}

impl GpuUsageSmoother {
    /// Creates an empty time-windowed GPU-usage smoother.
    pub(crate) fn new(window: Duration) -> Self {
        Self {
            window,
            samples: VecDeque::new(),
        }
    }

    /// Adds a measurement and returns its rolling-window average when available.
    pub(crate) fn add(&mut self, measurement: GpuMeasurement) -> GpuMeasurement {
        if measurement.status != TelemetryStatus::Available {
            self.samples.clear();
            return measurement;
        }

        if self
            .samples
            .back()
            .is_some_and(|(timestamp, _)| measurement.timestamp < *timestamp)
        {
            self.samples.clear();
        }
        while self.samples.front().is_some_and(|(timestamp, _)| {
            measurement.timestamp.saturating_duration_since(*timestamp) > self.window
        }) {
            self.samples.pop_front();
        }
        self.samples
            .push_back((measurement.timestamp, measurement.value));
        let average =
            self.samples.iter().map(|(_, value)| value).sum::<f32>() / self.samples.len() as f32;

        GpuMeasurement {
            value: average,
            ..measurement
        }
    }
}

struct SensorWindow {
    window: Duration,
    samples: VecDeque<(Instant, f32)>,
}

impl SensorWindow {
    fn new(window: Duration) -> Self {
        Self {
            window,
            samples: VecDeque::new(),
        }
    }

    fn add(&mut self, timestamp: Instant, value: Option<f32>) -> Option<f32> {
        let Some(value) = value.filter(|value| value.is_finite()) else {
            self.samples.clear();
            return None;
        };
        if self
            .samples
            .back()
            .is_some_and(|(sample_time, _)| timestamp < *sample_time)
        {
            self.samples.clear();
        }
        while self.samples.front().is_some_and(|(sample_time, _)| {
            timestamp.saturating_duration_since(*sample_time) > self.window
        }) {
            self.samples.pop_front();
        }
        self.samples.push_back((timestamp, value));
        Some(self.samples.iter().map(|(_, sample)| sample).sum::<f32>() / self.samples.len() as f32)
    }
}

pub(crate) struct HardwareMetricsSmoother {
    cpu_temp_c: SensorWindow,
    gpu_vrm_temp_c: SensorWindow,
    cpu_vrm_temp_c: SensorWindow,
    gpu_edge_temp_c: SensorWindow,
    cpu_load_percent: SensorWindow,
    gpu_power_w: SensorWindow,
    cpu_power_w: SensorWindow,
    ppt_w: SensorWindow,
    fan_rpm: SensorWindow,
    gtt_used_mb: SensorWindow,
    ram_used_mb: SensorWindow,
}

impl HardwareMetricsSmoother {
    pub(crate) fn new(window: Duration) -> Self {
        Self {
            cpu_temp_c: SensorWindow::new(window),
            gpu_vrm_temp_c: SensorWindow::new(window),
            cpu_vrm_temp_c: SensorWindow::new(window),
            gpu_edge_temp_c: SensorWindow::new(window),
            cpu_load_percent: SensorWindow::new(window),
            gpu_power_w: SensorWindow::new(window),
            cpu_power_w: SensorWindow::new(window),
            ppt_w: SensorWindow::new(window),
            fan_rpm: SensorWindow::new(window),
            gtt_used_mb: SensorWindow::new(window),
            ram_used_mb: SensorWindow::new(window),
        }
    }

    pub(crate) fn add(&mut self, metrics: HardwareMetrics, timestamp: Instant) -> HardwareMetrics {
        HardwareMetrics {
            cpu_temp_c: self.cpu_temp_c.add(timestamp, metrics.cpu_temp_c),
            gpu_vrm_temp_c: self.gpu_vrm_temp_c.add(timestamp, metrics.gpu_vrm_temp_c),
            cpu_vrm_temp_c: self.cpu_vrm_temp_c.add(timestamp, metrics.cpu_vrm_temp_c),
            gpu_edge_temp_c: self.gpu_edge_temp_c.add(timestamp, metrics.gpu_edge_temp_c),
            cpu_load_percent: self
                .cpu_load_percent
                .add(timestamp, metrics.cpu_load_percent),
            gpu_power_w: self.gpu_power_w.add(timestamp, metrics.gpu_power_w),
            cpu_power_w: self.cpu_power_w.add(timestamp, metrics.cpu_power_w),
            ppt_w: self.ppt_w.add(timestamp, metrics.ppt_w),
            fan_rpm: self.fan_rpm.add(timestamp, metrics.fan_rpm),
            gtt_used_mb: self.gtt_used_mb.add(timestamp, metrics.gtt_used_mb),
            gtt_total_mb: metrics.gtt_total_mb,
            ram_used_mb: self.ram_used_mb.add(timestamp, metrics.ram_used_mb),
            memory_temp: metrics.memory_temp,
        }
    }
}

pub trait GpuTelemetryProvider: Send + Sync {
    /// Reads the provider's latest GPU measurement.
    fn get_metric(&self) -> GpuMeasurement;
}

impl<T: GpuTelemetryProvider + ?Sized> GpuTelemetryProvider for Arc<T> {
    /// Forwards measurement requests through a shared provider reference.
    fn get_metric(&self) -> GpuMeasurement {
        (**self).get_metric()
    }
}

pub struct MockGpuProvider {
    pub mock_value: f32,
}

impl GpuTelemetryProvider for MockGpuProvider {
    /// Returns the configured mock value as a fresh available sample.
    fn get_metric(&self) -> GpuMeasurement {
        GpuMeasurement {
            value: self.mock_value,
            timestamp: Instant::now(),
            status: TelemetryStatus::Available,
            device_id: Some(0),
        }
    }
}

/// Fournisseur de télémétrie pour les cartes graphiques AMD.
/// Lit la charge GPU via les compteurs sysfs AMD disponibles.
pub struct AmdGpuProvider {
    pub card_path: String,
    device_id: Option<u32>,
}

impl AmdGpuProvider {
    /// Selects the sysfs usage counter for the requested DRM card index.
    pub fn new(card_index: u32) -> Self {
        let device_path = format!("/sys/class/drm/card{card_index}/device");
        let in_use_path = format!("{device_path}/in_use_percent");
        let busy_path = format!("{device_path}/gpu_busy_percent");
        Self {
            card_path: if std::path::Path::new(&in_use_path).is_file() {
                in_use_path
            } else if std::path::Path::new(&busy_path).is_file() {
                busy_path
            } else {
                in_use_path
            },
            device_id: Some(card_index),
        }
    }

    /// Finds a unique DRM card matching the given PCI vendor and device IDs.
    pub fn for_device(vendor_id: u32, device_id: u32) -> Self {
        let matches = std::fs::read_dir("/sys/class/drm")
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name();
                let name = name.to_str()?;
                let card_index = name.strip_prefix("card")?.parse::<u32>().ok()?;
                let device_path = entry.path().join("device");
                let vendor = read_pci_id(&device_path.join("vendor"))?;
                let device = read_pci_id(&device_path.join("device"))?;
                (vendor == vendor_id && device == device_id).then_some(card_index)
            })
            .collect::<Vec<_>>();
        if matches.len() == 1 {
            Self::new(matches[0])
        } else {
            Self {
                card_path: String::new(),
                device_id: None,
            }
        }
    }
}

/// Reads a hexadecimal PCI identifier from a sysfs attribute.
fn read_pci_id(path: &std::path::Path) -> Option<u32> {
    u32::from_str_radix(
        std::fs::read_to_string(path)
            .ok()?
            .trim()
            .strip_prefix("0x")?,
        16,
    )
    .ok()
}

impl GpuTelemetryProvider for AmdGpuProvider {
    /// Reads and validates the AMD GPU usage counter.
    fn get_metric(&self) -> GpuMeasurement {
        let contents = if self.card_path.is_empty() {
            Err(std::io::Error::from(std::io::ErrorKind::NotFound))
        } else {
            std::fs::read_to_string(&self.card_path)
        };
        match contents {
            Ok(content) => {
                let val_str = content.trim();
                let parsed = val_str.parse::<f32>().ok();
                if let Some(val) =
                    parsed.filter(|val| val.is_finite() && (0.0..=100.0).contains(val))
                {
                    GpuMeasurement {
                        value: val,
                        timestamp: Instant::now(),
                        status: TelemetryStatus::Available,
                        device_id: self.device_id,
                    }
                } else {
                    GpuMeasurement {
                        value: 0.0,
                        timestamp: Instant::now(),
                        status: TelemetryStatus::Error,
                        device_id: self.device_id,
                    }
                }
            }
            Err(_) => GpuMeasurement {
                value: 0.0,
                timestamp: Instant::now(),
                status: TelemetryStatus::Unsupported,
                device_id: self.device_id,
            },
        }
    }
}

pub(crate) struct MemoryUsageProvider {
    gtt_device: Option<PathBuf>,
    meminfo: PathBuf,
    gtt_used_failed: bool,
    gtt_total_failed: bool,
    ram_failed: bool,
}

impl MemoryUsageProvider {
    pub(crate) fn new(gpu: &AmdGpuProvider) -> Self {
        Self {
            gtt_device: (!gpu.card_path.is_empty())
                .then(|| Path::new(&gpu.card_path).parent().map(Path::to_path_buf))
                .flatten(),
            meminfo: PathBuf::from("/proc/meminfo"),
            gtt_used_failed: false,
            gtt_total_failed: false,
            ram_failed: false,
        }
    }

    pub(crate) fn update(&mut self, metrics: &mut HardwareMetrics) {
        let gtt_value = |name| -> anyhow::Result<f32> {
            let directory = self
                .gtt_device
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("GPU sysfs device is unavailable or ambiguous"))?;
            let path = directory.join(name);
            let bytes = fs::read_to_string(&path)
                .map_err(|error| anyhow::anyhow!("{}: {error}", path.display()))?
                .trim()
                .parse::<u64>()
                .map_err(|error| anyhow::anyhow!("{}: {error}", path.display()))?;
            Ok(bytes as f32 / 1_000_000.0)
        };
        let used = gtt_value("mem_info_gtt_used");
        let total = gtt_value("mem_info_gtt_total");
        let ram = fs::read_to_string(&self.meminfo)
            .map_err(anyhow::Error::from)
            .and_then(|contents| parse_ram_used_mb(&contents));
        metrics.gtt_used_mb = report_memory_reading("GTT used", used, &mut self.gtt_used_failed);
        metrics.gtt_total_mb =
            report_memory_reading("GTT total", total, &mut self.gtt_total_failed);
        metrics.ram_used_mb = report_memory_reading("RAM used", ram, &mut self.ram_failed);
    }
}

fn report_memory_reading(
    name: &str,
    reading: anyhow::Result<f32>,
    failed: &mut bool,
) -> Option<f32> {
    match reading {
        Ok(value) => {
            if *failed {
                diagnostics::info(format!("{name}: reading recovered"));
            }
            *failed = false;
            Some(value)
        }
        Err(error) => {
            if !*failed {
                diagnostics::warn(format!("{name}: {error:#}"));
            }
            *failed = true;
            None
        }
    }
}

fn parse_ram_used_mb(contents: &str) -> anyhow::Result<f32> {
    let mut total = None;
    let mut available = None;
    for line in contents.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let target = match name {
            "MemTotal" => &mut total,
            "MemAvailable" => &mut available,
            _ => continue,
        };
        if target.is_some() {
            anyhow::bail!("duplicate {name} in /proc/meminfo");
        }
        let mut fields = value.split_whitespace();
        let amount = fields
            .next()
            .ok_or_else(|| anyhow::anyhow!("missing {name} value"))?
            .parse::<u64>()?;
        if fields.next() != Some("kB") || fields.next().is_some() {
            anyhow::bail!("invalid {name} units in /proc/meminfo");
        }
        *target = Some(amount);
    }
    let total = total.ok_or_else(|| anyhow::anyhow!("MemTotal is unavailable"))?;
    let available = available.ok_or_else(|| anyhow::anyhow!("MemAvailable is unavailable"))?;
    let used = total
        .checked_sub(available)
        .ok_or_else(|| anyhow::anyhow!("MemAvailable exceeds MemTotal"))?;
    let bytes = used
        .checked_mul(1024)
        .ok_or_else(|| anyhow::anyhow!("RAM usage overflows bytes"))?;
    Ok(bytes as f32 / 1_000_000.0)
}

pub struct HwmonMetricsProvider {
    cpu_temp: Option<PathBuf>,
    gpu_vrm_temp: Option<PathBuf>,
    cpu_vrm_temp: Option<PathBuf>,
    gpu_edge_temp: Option<PathBuf>,
    gpu_power: Option<PathBuf>,
    cpu_power: Option<PathBuf>,
    ppt_power: Option<PathBuf>,
    fan_rpm: Option<PathBuf>,
}

impl HwmonMetricsProvider {
    /// Finds the requested sensors by their hwmon labels under the system hwmon root.
    pub fn new() -> Self {
        Self::with_paths(None, None, None, None, None, None, None, None)
    }

    /// Uses explicit sysfs paths where provided and discovers the remaining sensors by label.
    pub fn with_paths(
        cpu_temp: Option<PathBuf>,
        gpu_vrm_temp: Option<PathBuf>,
        cpu_vrm_temp: Option<PathBuf>,
        gpu_edge_temp: Option<PathBuf>,
        gpu_power: Option<PathBuf>,
        cpu_power: Option<PathBuf>,
        ppt_power: Option<PathBuf>,
        fan_rpm: Option<PathBuf>,
    ) -> Self {
        Self::from_root_with_paths(
            Path::new("/sys/class/hwmon"),
            cpu_temp,
            gpu_vrm_temp,
            cpu_vrm_temp,
            gpu_edge_temp,
            gpu_power,
            cpu_power,
            ppt_power,
            fan_rpm,
        )
    }

    pub fn with_settings(settings: &MetricsSettings) -> Self {
        Self::from_root_with_settings(Path::new("/sys/class/hwmon"), settings)
    }

    fn from_root_with_settings(root: &Path, settings: &MetricsSettings) -> Self {
        let mut provider = Self::from_root_with_paths(
            root,
            settings.cpu_temp_path.clone(),
            settings.gpu_vrm_temp_path.clone(),
            settings.cpu_vrm_temp_path.clone(),
            settings.gpu_edge_temp_path.clone(),
            settings.gpu_power_path.clone(),
            settings.cpu_power_path.clone(),
            settings.ppt_power_path.clone(),
            settings.fan_rpm_path.clone(),
        );
        for (name, sensor_type, selector, path, output) in [
            (
                "cpu_temp",
                "temp",
                &settings.cpu_temp,
                &settings.cpu_temp_path,
                &mut provider.cpu_temp,
            ),
            (
                "gpu_vrm_temp",
                "temp",
                &settings.gpu_vrm_temp,
                &settings.gpu_vrm_temp_path,
                &mut provider.gpu_vrm_temp,
            ),
            (
                "cpu_vrm_temp",
                "temp",
                &settings.cpu_vrm_temp,
                &settings.cpu_vrm_temp_path,
                &mut provider.cpu_vrm_temp,
            ),
            (
                "gpu_edge_temp",
                "temp",
                &settings.gpu_edge_temp,
                &settings.gpu_edge_temp_path,
                &mut provider.gpu_edge_temp,
            ),
            (
                "gpu_power",
                "power",
                &settings.gpu_power,
                &settings.gpu_power_path,
                &mut provider.gpu_power,
            ),
            (
                "cpu_power",
                "power",
                &settings.cpu_power,
                &settings.cpu_power_path,
                &mut provider.cpu_power,
            ),
            (
                "ppt_power",
                "power",
                &settings.ppt_power,
                &settings.ppt_power_path,
                &mut provider.ppt_power,
            ),
            (
                "fan_rpm",
                "fan",
                &settings.fan_rpm,
                &settings.fan_rpm_path,
                &mut provider.fan_rpm,
            ),
        ] {
            if let Some(selector) = selector {
                if path.is_some() {
                    diagnostics::warn(format!(
                        "sensor {name}: both path and device/label configured; sensor disabled"
                    ));
                    *output = None;
                } else {
                    *output = find_selected_sensor(root, sensor_type, name, selector);
                }
            } else if let Some(path) = path {
                if resolve_sensor_path(path).is_none() {
                    diagnostics::warn(format!(
                        "sensor {name}: configured path {} not found; using label discovery",
                        path.display()
                    ));
                }
            }
        }
        provider
    }

    fn from_root_with_paths(
        root: &Path,
        cpu_temp: Option<PathBuf>,
        gpu_vrm_temp: Option<PathBuf>,
        cpu_vrm_temp: Option<PathBuf>,
        gpu_edge_temp: Option<PathBuf>,
        gpu_power: Option<PathBuf>,
        cpu_power: Option<PathBuf>,
        ppt_power: Option<PathBuf>,
        fan_rpm: Option<PathBuf>,
    ) -> Self {
        let directories = fs::read_dir(root)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .collect::<Vec<_>>();

        Self {
            cpu_temp: cpu_temp
                .and_then(|path| resolve_sensor_path(&path))
                .or_else(|| {
                    find_labeled_sensor(&directories, "temp", "Tctl", "input")
                        .or_else(|| find_labeled_sensor(&directories, "temp", "Tdie", "input"))
                        .or_else(|| {
                            find_labeled_sensor(&directories, "temp", "Package id 0", "input")
                        })
                }),
            gpu_vrm_temp: gpu_vrm_temp
                .and_then(|path| resolve_sensor_path(&path))
                .or_else(|| find_labeled_sensor(&directories, "temp", "GPU VRM Temp", "input")),
            cpu_vrm_temp: cpu_vrm_temp
                .and_then(|path| resolve_sensor_path(&path))
                .or_else(|| find_labeled_sensor(&directories, "temp", "CPU VRM Temp", "input")),
            gpu_edge_temp: gpu_edge_temp
                .and_then(|path| resolve_sensor_path(&path))
                .or_else(|| find_labeled_sensor(&directories, "temp", "edge", "input")),
            gpu_power: gpu_power
                .and_then(|path| resolve_sensor_path(&path))
                .or_else(|| find_labeled_sensor(&directories, "power", "GPU Power", "input")),
            cpu_power: cpu_power
                .and_then(|path| resolve_sensor_path(&path))
                .or_else(|| find_labeled_sensor(&directories, "power", "CPU Power", "input")),
            ppt_power: ppt_power
                .and_then(|path| resolve_sensor_path(&path))
                .or_else(|| {
                    find_labeled_sensor(&directories, "power", "PPT", "average")
                        .or_else(|| find_labeled_sensor(&directories, "power", "PPT", "input"))
                }),
            fan_rpm: fan_rpm
                .and_then(|path| resolve_sensor_path(&path))
                .or_else(|| find_labeled_sensor(&directories, "fan", "CPU Fan", "input")),
        }
    }

    /// Reads the discovered sensors and converts hwmon's millidegrees/microwatts to C/W.
    pub fn get_metrics(&self) -> HardwareMetrics {
        HardwareMetrics {
            cpu_temp_c: read_hwmon_value(self.cpu_temp.as_deref(), 1_000.0),
            gpu_vrm_temp_c: read_hwmon_value(self.gpu_vrm_temp.as_deref(), 1_000.0),
            cpu_vrm_temp_c: read_hwmon_value(self.cpu_vrm_temp.as_deref(), 1_000.0),
            gpu_edge_temp_c: read_hwmon_value(self.gpu_edge_temp.as_deref(), 1_000.0),
            gpu_power_w: read_hwmon_value(self.gpu_power.as_deref(), 1_000_000.0),
            cpu_power_w: read_hwmon_value(self.cpu_power.as_deref(), 1_000_000.0),
            ppt_w: read_hwmon_value(self.ppt_power.as_deref(), 1_000_000.0),
            fan_rpm: read_hwmon_value(self.fan_rpm.as_deref(), 1.0),
            ..HardwareMetrics::default()
        }
    }
}

fn find_selected_sensor(
    root: &Path,
    sensor_type: &str,
    name: &str,
    selector: &SensorSelector,
) -> Option<PathBuf> {
    let directories = match fs::read_dir(root) {
        Ok(directories) => directories,
        Err(error) => {
            diagnostics::warn(format!(
                "sensor {name}: cannot read {}: {error}",
                root.display()
            ));
            return None;
        }
    };
    let mut matches = Vec::new();
    for entry in directories {
        let directory = match entry {
            Ok(entry) => entry.path(),
            Err(error) => {
                diagnostics::warn(format!(
                    "sensor {name}: cannot enumerate {}: {error}",
                    root.display()
                ));
                return None;
            }
        };
        if fs::read_to_string(directory.join("name"))
            .is_ok_and(|device| device.trim() == selector.device)
        {
            let entries = match fs::read_dir(&directory) {
                Ok(entries) => entries,
                Err(error) => {
                    diagnostics::warn(format!(
                        "sensor {name}: cannot enumerate {}: {error}",
                        directory.display()
                    ));
                    return None;
                }
            };
            for entry in entries {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        diagnostics::warn(format!(
                            "sensor {name}: cannot enumerate {}: {error}",
                            directory.display()
                        ));
                        return None;
                    }
                };
                let filename = entry.file_name();
                let Some(index) = filename.to_str().and_then(|filename| {
                    filename.strip_prefix(sensor_type)?.strip_suffix("_label")
                }) else {
                    continue;
                };
                match fs::read_to_string(entry.path()) {
                    Ok(label) if label.trim() == selector.label => {
                        let base = directory.join(format!("{sensor_type}{index}"));
                        if let Some(path) = resolve_sensor_path(&base) {
                            matches.push(path);
                        }
                    }
                    Ok(_) => {}
                    Err(error) => {
                        diagnostics::warn(format!(
                            "sensor {name}: cannot read {}: {error}",
                            entry.path().display()
                        ));
                        return None;
                    }
                }
            }
        }
    }
    match matches.as_slice() {
        [path] => {
            diagnostics::info(format!("sensor {name}: selected {}", path.display()));
            Some(path.clone())
        }
        _ => {
            diagnostics::warn(format!("sensor {name}: device {:?}, label {:?} matched {} usable sensors; sensor unavailable", selector.device, selector.label, matches.len()));
            None
        }
    }
}

fn resolve_sensor_path(path: &Path) -> Option<PathBuf> {
    if path.is_file() {
        return Some(path.to_path_buf());
    }

    ["_input", "_average"].into_iter().find_map(|suffix| {
        let mut candidate = path.as_os_str().to_os_string();
        candidate.push(suffix);
        let candidate = PathBuf::from(candidate);
        candidate.is_file().then_some(candidate)
    })
}

fn find_labeled_sensor(
    directories: &[PathBuf],
    sensor_type: &str,
    label: &str,
    value_suffix: &str,
) -> Option<PathBuf> {
    directories.iter().find_map(|directory| {
        fs::read_dir(directory).ok()?.flatten().find_map(|entry| {
            let filename = entry.file_name();
            let filename = filename.to_str()?;
            let index = filename.strip_prefix(sensor_type)?.strip_suffix("_label")?;
            let label_contents = fs::read_to_string(entry.path()).ok()?;
            if label_contents.trim() == label {
                let value_path = directory.join(format!("{sensor_type}{index}_{value_suffix}"));
                value_path.is_file().then_some(value_path)
            } else {
                None
            }
        })
    })
}

fn read_hwmon_value(path: Option<&Path>, divisor: f32) -> Option<f32> {
    let value = fs::read_to_string(path?).ok()?.trim().parse::<f32>().ok()?;
    value.is_finite().then_some(value / divisor)
}

pub(crate) struct CpuUsageProvider {
    proc_stat_path: PathBuf,
    previous: Option<(u64, u64)>,
}

impl CpuUsageProvider {
    pub(crate) fn new() -> Self {
        Self::from_path(PathBuf::from("/proc/stat"))
    }

    fn from_path(proc_stat_path: PathBuf) -> Self {
        Self {
            proc_stat_path,
            previous: None,
        }
    }

    /// Returns CPU utilization since the preceding `/proc/stat` sample.
    pub(crate) fn get_load_percent(&mut self) -> Option<f32> {
        let contents = fs::read_to_string(&self.proc_stat_path).ok()?;
        let (total, idle) = parse_cpu_stat(&contents)?;
        let previous = self.previous.replace((total, idle));
        let (previous_total, previous_idle) = previous?;
        let total_delta = total.checked_sub(previous_total)?;
        let idle_delta = idle.checked_sub(previous_idle)?;
        if total_delta == 0 {
            return None;
        }
        Some((1.0 - idle_delta as f32 / total_delta as f32).clamp(0.0, 1.0) * 100.0)
    }
}

fn parse_cpu_stat(contents: &str) -> Option<(u64, u64)> {
    let mut fields = contents.lines().next()?.split_whitespace();
    if fields.next()? != "cpu" {
        return None;
    }
    let counters = fields
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if counters.len() < 5 {
        return None;
    }
    let total = counters
        .iter()
        .take(8)
        .try_fold(0u64, |total, counter| total.checked_add(*counter))?;
    let idle = counters[3].checked_add(counters[4])?;
    Some((total, idle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_DIRECTORY_ID: AtomicU64 = AtomicU64::new(0);

    fn write_sensor_file(root: &Path, path: &str, value: &str) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, value).unwrap();
    }

    fn temporary_hwmon_root() -> PathBuf {
        let id = TEST_DIRECTORY_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("framecap-hwmon-{}-{id}", std::process::id()))
    }

    /// Creates an available test measurement with a controlled timestamp.
    fn available(value: f32, timestamp: Instant) -> GpuMeasurement {
        GpuMeasurement {
            value,
            timestamp,
            status: TelemetryStatus::Available,
            device_id: Some(0),
        }
    }

    #[test]
    fn parses_system_ram_usage_in_decimal_megabytes() {
        assert_eq!(
            parse_ram_used_mb("MemTotal: 1000000 kB\nMemAvailable: 500000 kB\n").unwrap(),
            512.0
        );
        for invalid in [
            "MemTotal: 1 kB\n",
            "MemTotal: 1 kB\nMemAvailable: 2 kB\n",
            "MemTotal: 1 MB\nMemAvailable: 1 kB\n",
            "MemTotal: 1 kB\nMemTotal: 1 kB\nMemAvailable: 1 kB\n",
            "MemTotal: 18446744073709551615 kB\nMemAvailable: 0 kB\n",
        ] {
            assert!(parse_ram_used_mb(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn memory_usage_reads_each_source_independently() {
        let root = temporary_hwmon_root();
        write_sensor_file(&root, "device/mem_info_gtt_used", "512000000\n");
        write_sensor_file(&root, "device/mem_info_gtt_total", "12000000000\n");
        write_sensor_file(
            &root,
            "meminfo",
            "MemTotal: 5000000 kB\nMemAvailable: 1000000 kB\n",
        );
        let mut provider = MemoryUsageProvider {
            gtt_device: Some(root.join("device")),
            meminfo: root.join("meminfo"),
            gtt_used_failed: false,
            gtt_total_failed: false,
            ram_failed: false,
        };
        let mut metrics = HardwareMetrics::default();
        provider.update(&mut metrics);
        assert_eq!(metrics.gtt_used_mb, Some(512.0));
        assert_eq!(metrics.gtt_total_mb, Some(12000.0));
        assert_eq!(metrics.ram_used_mb, Some(4096.0));
        fs::write(root.join("device/mem_info_gtt_used"), "invalid").unwrap();
        fs::remove_file(root.join("meminfo")).unwrap();
        provider.update(&mut metrics);
        assert_eq!(metrics.gtt_used_mb, None);
        assert_eq!(metrics.gtt_total_mb, Some(12000.0));
        assert_eq!(metrics.ram_used_mb, None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn memory_usage_smoothing_is_independent_and_total_is_current() {
        let now = Instant::now();
        let mut smoother = HardwareMetricsSmoother::new(Duration::from_secs(1));
        smoother.add(
            HardwareMetrics {
                gtt_used_mb: Some(100.0),
                gtt_total_mb: Some(1000.0),
                ram_used_mb: Some(400.0),
                ..HardwareMetrics::default()
            },
            now,
        );
        let result = smoother.add(
            HardwareMetrics {
                gtt_used_mb: Some(200.0),
                gtt_total_mb: Some(1200.0),
                ram_used_mb: None,
                ..HardwareMetrics::default()
            },
            now + Duration::from_millis(500),
        );
        assert_eq!(result.gtt_used_mb, Some(150.0));
        assert_eq!(result.gtt_total_mb, Some(1200.0));
        assert_eq!(result.ram_used_mb, None);
        let recovered = smoother.add(
            HardwareMetrics {
                gtt_used_mb: None,
                gtt_total_mb: Some(1200.0),
                ram_used_mb: Some(800.0),
                ..HardwareMetrics::default()
            },
            now + Duration::from_millis(600),
        );
        assert_eq!(recovered.gtt_used_mb, None);
        assert_eq!(recovered.ram_used_mb, Some(800.0));
    }

    #[test]
    /// Checks averaging and expiry of GPU samples within the configured window.
    fn gpu_usage_is_averaged_over_the_configured_rolling_window() {
        let start = Instant::now();
        let mut smoother = GpuUsageSmoother::new(Duration::from_secs(1));

        assert_eq!(smoother.add(available(0.0, start)).value, 0.0);
        assert_eq!(
            smoother
                .add(available(100.0, start + Duration::from_millis(100)))
                .value,
            50.0
        );
        assert_eq!(
            smoother
                .add(available(0.0, start + Duration::from_millis(200)))
                .value,
            100.0 / 3.0
        );
        assert_eq!(
            smoother
                .add(available(100.0, start + Duration::from_millis(1200)))
                .value,
            50.0
        );
    }

    #[test]
    /// Checks that an unavailable sample clears the smoothing history.
    fn unavailable_gpu_measurement_clears_the_smoothing_window() {
        let start = Instant::now();
        let mut smoother = GpuUsageSmoother::new(Duration::from_secs(1));
        smoother.add(available(100.0, start));

        let unavailable = GpuMeasurement {
            value: 0.0,
            timestamp: start + Duration::from_millis(100),
            status: TelemetryStatus::Unsupported,
            device_id: Some(0),
        };
        assert_eq!(
            smoother.add(unavailable.clone()).status,
            TelemetryStatus::Unsupported
        );
        assert_eq!(
            smoother
                .add(available(20.0, start + Duration::from_millis(200)))
                .value,
            20.0
        );
    }

    #[test]
    /// Discovers hwmon values by label and normalizes temperatures and power units.
    fn hwmon_metrics_are_discovered_and_converted() {
        let root = temporary_hwmon_root();
        write_sensor_file(&root, "hwmon4/temp1_label", "CPU VRM Temp\n");
        write_sensor_file(&root, "hwmon4/temp1_input", "61000\n");
        write_sensor_file(&root, "hwmon4/temp2_label", "GPU VRM Temp\n");
        write_sensor_file(&root, "hwmon4/temp2_input", "57000\n");
        write_sensor_file(&root, "hwmon1/temp1_label", "edge\n");
        write_sensor_file(&root, "hwmon1/temp1_input", "43000\n");
        write_sensor_file(&root, "hwmon1/power1_label", "PPT\n");
        write_sensor_file(&root, "hwmon1/power1_average", "32610000\n");
        write_sensor_file(&root, "hwmon2/temp1_label", "Tctl\n");
        write_sensor_file(&root, "hwmon2/temp1_input", "70000\n");
        write_sensor_file(&root, "hwmon4/power1_label", "CPU Power\n");
        write_sensor_file(&root, "hwmon4/power1_input", "5200000\n");
        write_sensor_file(&root, "hwmon4/power2_label", "GPU Power\n");
        write_sensor_file(&root, "hwmon4/power2_input", "5800000\n");
        write_sensor_file(&root, "hwmon3/fan1_label", "CPU Fan\n");
        write_sensor_file(&root, "hwmon3/fan1_input", "1240\n");
        write_sensor_file(&root, "hwmon1/power1_input", "40120000\n");

        let metrics = HwmonMetricsProvider::from_root_with_paths(
            &root, None, None, None, None, None, None, None, None,
        )
        .get_metrics();

        assert_eq!(metrics.cpu_temp_c, Some(70.0));
        assert_eq!(metrics.gpu_vrm_temp_c, Some(57.0));
        assert_eq!(metrics.cpu_vrm_temp_c, Some(61.0));
        assert_eq!(metrics.gpu_edge_temp_c, Some(43.0));
        assert_eq!(metrics.gpu_power_w, Some(5.8));
        assert_eq!(metrics.cpu_power_w, Some(5.2));
        assert_eq!(metrics.ppt_w, Some(32.61));
        assert_eq!(metrics.fan_rpm, Some(1240.0));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    /// Checks configured paths take precedence over automatic label discovery.
    fn configured_hwmon_paths_override_discovered_sensors() {
        let root = temporary_hwmon_root();
        write_sensor_file(&root, "hwmon4/temp1_label", "GPU VRM Temp\n");
        write_sensor_file(&root, "hwmon4/temp1_input", "57000\n");
        write_sensor_file(&root, "custom/gpu-temp", "65000\n");

        let metrics = HwmonMetricsProvider::from_root_with_paths(
            &root,
            None,
            Some(root.join("custom/gpu-temp")),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .get_metrics();

        assert_eq!(metrics.gpu_vrm_temp_c, Some(65.0));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    /// Resolves power sensor base paths to their sysfs `_input` attributes.
    fn configured_sensor_base_path_resolves_input_attribute() {
        let root = temporary_hwmon_root();
        write_sensor_file(&root, "hwmon4/power2_input", "5800000\n");

        assert_eq!(
            resolve_sensor_path(&root.join("hwmon4/power2")),
            Some(root.join("hwmon4/power2_input"))
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn configured_sensor_selector_survives_renumbering_and_rejects_ambiguity() {
        let root = temporary_hwmon_root();
        write_sensor_file(&root, "hwmon3/name", "k10temp\n");
        write_sensor_file(&root, "hwmon2/name", "nct6686\n");
        write_sensor_file(&root, "hwmon2/fan1_label", "CPU Fan\n");
        write_sensor_file(&root, "hwmon2/fan1_input", "0\n");
        write_sensor_file(&root, "hwmon2/fan2_label", "Pump Fan\n");
        write_sensor_file(&root, "hwmon2/fan2_input", "780\n");
        let mut settings = MetricsSettings {
            fan_rpm: Some(SensorSelector {
                device: "nct6686".into(),
                label: "Pump Fan".into(),
            }),
            ..MetricsSettings::default()
        };
        assert_eq!(
            HwmonMetricsProvider::from_root_with_settings(&root, &settings)
                .get_metrics()
                .fan_rpm,
            Some(780.0)
        );
        fs::rename(root.join("hwmon2"), root.join("hwmon7")).unwrap();
        assert_eq!(
            HwmonMetricsProvider::from_root_with_settings(&root, &settings)
                .get_metrics()
                .fan_rpm,
            Some(780.0)
        );
        write_sensor_file(&root, "hwmon8/name", "nct6686\n");
        write_sensor_file(&root, "hwmon8/fan1_label", "Pump Fan\n");
        write_sensor_file(&root, "hwmon8/fan1_input", "900\n");
        assert_eq!(
            HwmonMetricsProvider::from_root_with_settings(&root, &settings)
                .get_metrics()
                .fan_rpm,
            None
        );
        settings.fan_rpm.as_mut().unwrap().label = "Missing Fan".into();
        assert_eq!(
            HwmonMetricsProvider::from_root_with_settings(&root, &settings)
                .get_metrics()
                .fan_rpm,
            None
        );
        settings.fan_rpm_path = Some(root.join("hwmon7/fan2_input"));
        assert_eq!(
            HwmonMetricsProvider::from_root_with_settings(&root, &settings)
                .get_metrics()
                .fan_rpm,
            None
        );
        settings.fan_rpm = None;
        assert_eq!(
            HwmonMetricsProvider::from_root_with_settings(&root, &settings)
                .get_metrics()
                .fan_rpm,
            Some(780.0)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn selected_power_prefers_input_and_falls_back_to_average() {
        let root = temporary_hwmon_root();
        write_sensor_file(&root, "hwmon1/name", "amdgpu\n");
        write_sensor_file(&root, "hwmon1/power1_label", "PPT\n");
        write_sensor_file(&root, "hwmon1/power1_input", "40000000\n");
        write_sensor_file(&root, "hwmon1/power1_average", "30000000\n");
        let settings = MetricsSettings {
            ppt_power: Some(SensorSelector {
                device: "amdgpu".into(),
                label: "PPT".into(),
            }),
            ..MetricsSettings::default()
        };
        assert_eq!(
            HwmonMetricsProvider::from_root_with_settings(&root, &settings)
                .get_metrics()
                .ppt_w,
            Some(40.0)
        );
        fs::remove_file(root.join("hwmon1/power1_input")).unwrap();
        assert_eq!(
            HwmonMetricsProvider::from_root_with_settings(&root, &settings)
                .get_metrics()
                .ppt_w,
            Some(30.0)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    /// Computes CPU utilization from consecutive aggregate `/proc/stat` samples.
    fn cpu_usage_uses_idle_and_total_counter_deltas() {
        let root = temporary_hwmon_root();
        let stat_path = root.join("stat");
        fs::create_dir_all(&root).unwrap();
        fs::write(&stat_path, "cpu 100 0 50 850 0 0 0 0\n").unwrap();
        let mut provider = CpuUsageProvider::from_path(stat_path.clone());

        assert_eq!(provider.get_load_percent(), None);
        fs::write(&stat_path, "cpu 150 0 70 880 0 0 0 0\n").unwrap();
        assert!((provider.get_load_percent().unwrap() - 70.0).abs() < 0.01);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    /// Smooths each sensor independently across the configured time window.
    fn hardware_metrics_are_smoothed_over_the_configured_window() {
        let start = Instant::now();
        let mut smoother = HardwareMetricsSmoother::new(Duration::from_millis(100));
        let first = smoother.add(
            HardwareMetrics {
                gpu_edge_temp_c: Some(40.0),
                gpu_power_w: Some(100.0),
                cpu_load_percent: Some(20.0),
                ppt_w: Some(10.0),
                fan_rpm: Some(1000.0),
                ..HardwareMetrics::default()
            },
            start,
        );
        assert_eq!(first.gpu_edge_temp_c, Some(40.0));

        let second = smoother.add(
            HardwareMetrics {
                gpu_edge_temp_c: Some(60.0),
                gpu_power_w: Some(200.0),
                cpu_load_percent: Some(40.0),
                ppt_w: Some(30.0),
                fan_rpm: Some(2000.0),
                ..HardwareMetrics::default()
            },
            start + Duration::from_millis(50),
        );
        assert_eq!(second.gpu_edge_temp_c, Some(50.0));
        assert_eq!(second.gpu_power_w, Some(150.0));
        assert_eq!(second.cpu_load_percent, Some(30.0));
        assert_eq!(second.ppt_w, Some(20.0));
        assert_eq!(second.fan_rpm, Some(1500.0));

        let expired = smoother.add(
            HardwareMetrics {
                gpu_edge_temp_c: Some(80.0),
                gpu_power_w: Some(300.0),
                cpu_load_percent: Some(60.0),
                ppt_w: Some(50.0),
                fan_rpm: Some(3000.0),
                ..HardwareMetrics::default()
            },
            start + Duration::from_millis(200),
        );
        assert_eq!(expired.gpu_edge_temp_c, Some(80.0));
        assert_eq!(expired.gpu_power_w, Some(300.0));
        assert_eq!(expired.cpu_load_percent, Some(60.0));
        assert_eq!(expired.ppt_w, Some(50.0));
        assert_eq!(expired.fan_rpm, Some(3000.0));
    }

    #[test]
    /// Clears only the history of a sensor whose current reading is unavailable.
    fn unavailable_sensor_clears_only_its_smoothing_history() {
        let start = Instant::now();
        let mut smoother = HardwareMetricsSmoother::new(Duration::from_secs(1));
        smoother.add(
            HardwareMetrics {
                gpu_edge_temp_c: Some(40.0),
                cpu_power_w: Some(100.0),
                ..HardwareMetrics::default()
            },
            start,
        );
        let result = smoother.add(
            HardwareMetrics {
                cpu_power_w: Some(200.0),
                ..HardwareMetrics::default()
            },
            start + Duration::from_millis(50),
        );
        assert_eq!(result.gpu_edge_temp_c, None);
        assert_eq!(result.cpu_power_w, Some(150.0));
    }
}

pub struct TelemetryManager {
    pub provider: Box<dyn GpuTelemetryProvider>,
    pub last_measurement: Arc<RwLock<GpuMeasurement>>,
    pub expiry_ms: u64,
}

impl TelemetryManager {
    /// Creates a manager seeded with an initial provider measurement.
    pub fn new(provider: Box<dyn GpuTelemetryProvider>, expiry_ms: u64) -> Self {
        let initial_measurement = provider.get_metric();
        Self {
            provider,
            last_measurement: Arc::new(RwLock::new(initial_measurement)),
            expiry_ms,
        }
    }

    /// Polls the provider periodically and publishes each measurement.
    pub async fn run_polling_loop(&self, poll_rate_ms: u64) {
        let mut interval = tokio::time::interval(Duration::from_millis(poll_rate_ms));
        loop {
            interval.tick().await;
            let new_metric = self.provider.get_metric();
            {
                let mut lock = self.last_measurement.write().unwrap();
                *lock = new_metric;
            }
        }
    }

    /// Returns the last measurement, marking it expired when too old.
    pub fn get_current_measurement(&self) -> GpuMeasurement {
        let lock = self.last_measurement.read().unwrap();
        let measurement = lock.clone();

        if measurement.status == TelemetryStatus::Available
            && measurement.timestamp.elapsed().as_millis() < self.expiry_ms as u128
        {
            measurement
        } else {
            GpuMeasurement {
                value: 0.0,
                timestamp: measurement.timestamp,
                status: TelemetryStatus::Expired,
                device_id: measurement.device_id,
            }
        }
    }
}
