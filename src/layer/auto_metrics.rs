use crate::layer::config::AutoMetric;
use crate::layer::lib::{GpuMeasurement, HardwareMetrics, MetricsSnapshot, TelemetryStatus};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
struct Sample {
    timestamp: Instant,
    value: f32,
}

#[derive(Clone, Copy)]
struct PresentationSample {
    timestamp: Instant,
    frametime_ms: f32,
    fps: f32,
}

/// Time-based metrics history. Keep one instance per auto algorithm and swapchain.
pub struct AutoMetricsHistory {
    window: Duration,
    gpu: VecDeque<Sample>,
    gpu_since: Option<Instant>,
    vrm: VecDeque<Sample>,
    vrm_since: Option<Instant>,
    fan: VecDeque<Sample>,
    fan_since: Option<Instant>,
    presentations: VecDeque<PresentationSample>,
    presentations_since: Option<Instant>,
    last_presentation: Option<Instant>,
    latest_gpu: Option<GpuMeasurement>,
}

impl AutoMetricsHistory {
    /// Creates an empty history with the requested rolling time window.
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            gpu: VecDeque::new(),
            gpu_since: None,
            vrm: VecDeque::new(),
            vrm_since: None,
            fan: VecDeque::new(),
            fan_since: None,
            presentations: VecDeque::new(),
            presentations_since: None,
            last_presentation: None,
            latest_gpu: None,
        }
    }

    /// Records raw telemetry and one presentation. Telemetry timestamps are deduplicated.
    pub fn record(
        &mut self,
        now: Instant,
        gpu: &GpuMeasurement,
        hardware: &HardwareMetrics,
        hardware_timestamp: Instant,
    ) {
        Self::evict(&mut self.gpu, &mut self.gpu_since, self.window, now);
        Self::evict(&mut self.vrm, &mut self.vrm_since, self.window, now);
        Self::evict(&mut self.fan, &mut self.fan_since, self.window, now);
        self.evict_presentations(now);
        let gpu_valid = gpu.status == TelemetryStatus::Available
            && gpu.value.is_finite()
            && (0.0..=100.0).contains(&gpu.value)
            && gpu.timestamp <= now
            && now.saturating_duration_since(gpu.timestamp) <= self.window;

        if gpu_valid {
            if Self::record_sample(
                &mut self.gpu,
                &mut self.gpu_since,
                Sample {
                    timestamp: gpu.timestamp,
                    value: gpu.value,
                },
                self.window,
            ) {
                self.latest_gpu = Some(gpu.clone());
            }
        } else {
            self.gpu.clear();
            self.gpu_since = None;
            self.latest_gpu = (gpu.timestamp <= now).then(|| gpu.clone());
        }

        if gpu_valid {
            if let Some(previous) = self.last_presentation {
                if now > previous {
                    let elapsed = now.duration_since(previous);
                    if elapsed > self.window {
                        self.presentations.clear();
                        self.presentations_since = None;
                    } else {
                        let frametime_ms = (elapsed.as_secs_f64() * 1000.0) as f32;
                        if frametime_ms.is_finite() && frametime_ms > 0.0 {
                            Self::record_presentation(
                                &mut self.presentations,
                                &mut self.presentations_since,
                                PresentationSample {
                                    timestamp: now,
                                    frametime_ms,
                                    fps: 1000.0 / frametime_ms,
                                },
                                self.window,
                            );
                        } else {
                            self.presentations.clear();
                            self.presentations_since = None;
                        }
                    }
                } else if now < previous {
                    self.presentations.clear();
                    self.presentations_since = None;
                    self.last_presentation = Some(now);
                }
            }
            if self.last_presentation.is_none_or(|last| now > last) {
                self.last_presentation = Some(now);
            }
        } else {
            self.presentations.clear();
            self.presentations_since = None;
            self.last_presentation = None;
        }

        if hardware_timestamp > now
            || now.saturating_duration_since(hardware_timestamp) > self.window
        {
            self.vrm.clear();
            self.vrm_since = None;
            self.fan.clear();
            self.fan_since = None;
            return;
        }

        match hardware.gpu_vrm_temp_c.filter(|value| value.is_finite()) {
            Some(value) => {
                Self::record_sample(
                    &mut self.vrm,
                    &mut self.vrm_since,
                    Sample {
                        timestamp: hardware_timestamp,
                        value,
                    },
                    self.window,
                );
            }
            None => {
                self.vrm.clear();
                self.vrm_since = None;
            }
        }
        match hardware
            .fan_rpm
            .filter(|value| value.is_finite() && *value >= 0.0)
        {
            Some(value) => {
                Self::record_sample(
                    &mut self.fan,
                    &mut self.fan_since,
                    Sample {
                        timestamp: hardware_timestamp,
                        value,
                    },
                    self.window,
                );
            }
            None => {
                self.fan.clear();
                self.fan_since = None;
            }
        }
    }

    /// Returns a snapshot only after the selected metric has a complete, fresh window.
    pub fn snapshot(&mut self, now: Instant, metric: AutoMetric) -> Option<MetricsSnapshot> {
        Self::evict(&mut self.gpu, &mut self.gpu_since, self.window, now);
        Self::evict(&mut self.vrm, &mut self.vrm_since, self.window, now);
        Self::evict(&mut self.fan, &mut self.fan_since, self.window, now);
        self.evict_presentations(now);

        let (frametime_ms, fps, jitter_ms, low_10_fps) = if self.complete_presentations(now) {
            let latest = self.presentations.back()?;
            (
                latest.frametime_ms,
                (self
                    .presentations
                    .iter()
                    .map(|sample| sample.fps as f64)
                    .sum::<f64>()
                    / self.presentations.len() as f64) as f32,
                self.jitter_ms(),
                self.low_10_fps(),
            )
        } else {
            (-1.0, -1.0, -1.0, -1.0)
        };

        let mut hardware = HardwareMetrics::default();
        if self.complete_samples(&self.vrm, self.vrm_since, now) {
            hardware.gpu_vrm_temp_c = Some(Self::average(&self.vrm));
        }
        if self.complete_samples(&self.fan, self.fan_since, now) {
            hardware.fan_rpm = Some(Self::average(&self.fan));
        }

        let selected_timestamp = match metric {
            AutoMetric::Fps => {
                if !self.complete_presentations(now) {
                    return None;
                }
                if !self.complete_samples(&self.gpu, self.gpu_since, now) {
                    return None;
                }
                self.gpu.back()?.timestamp
            }
            AutoMetric::GpuVrmTempC => {
                if !self.complete_samples(&self.vrm, self.vrm_since, now) {
                    return None;
                }
                self.vrm.back()?.timestamp
            }
            AutoMetric::FanRpm => {
                if !self.complete_samples(&self.fan, self.fan_since, now) {
                    return None;
                }
                self.fan.back()?.timestamp
            }
        };

        let gpu = if self.complete_samples(&self.gpu, self.gpu_since, now) {
            let mut measurement = self.latest_gpu.clone()?;
            measurement.value = Self::average(&self.gpu);
            measurement.timestamp = self.gpu.back()?.timestamp;
            measurement.status = TelemetryStatus::Available;
            measurement
        } else {
            self.latest_gpu.clone().unwrap_or(GpuMeasurement {
                value: -1.0,
                timestamp: now,
                status: TelemetryStatus::Expired,
                device_id: None,
            })
        };
        Some(MetricsSnapshot::new_with_hardware_metrics_at(
            frametime_ms,
            fps,
            jitter_ms,
            low_10_fps,
            gpu,
            hardware,
            selected_timestamp,
            now,
        ))
    }

    fn record_sample(
        samples: &mut VecDeque<Sample>,
        since: &mut Option<Instant>,
        sample: Sample,
        window: Duration,
    ) -> bool {
        if samples
            .back()
            .is_some_and(|last| sample.timestamp <= last.timestamp)
        {
            return false;
        }
        if samples
            .back()
            .is_some_and(|last| sample.timestamp.duration_since(last.timestamp) > window)
        {
            samples.clear();
            *since = None;
        }
        since.get_or_insert(sample.timestamp);
        samples.push_back(sample);
        true
    }

    fn record_presentation(
        samples: &mut VecDeque<PresentationSample>,
        since: &mut Option<Instant>,
        sample: PresentationSample,
        window: Duration,
    ) -> bool {
        if samples
            .back()
            .is_some_and(|last| sample.timestamp <= last.timestamp)
        {
            return false;
        }
        if samples
            .back()
            .is_some_and(|last| sample.timestamp.duration_since(last.timestamp) > window)
        {
            samples.clear();
            *since = None;
        }
        since.get_or_insert(sample.timestamp);
        samples.push_back(sample);
        true
    }

    fn evict(
        samples: &mut VecDeque<Sample>,
        since: &mut Option<Instant>,
        window: Duration,
        now: Instant,
    ) {
        while samples.front().is_some_and(|sample| {
            sample.timestamp > now || now.saturating_duration_since(sample.timestamp) > window
        }) {
            samples.pop_front();
        }
        if samples.is_empty() {
            *since = None;
        }
    }

    fn evict_presentations(&mut self, now: Instant) {
        while self.presentations.front().is_some_and(|sample| {
            sample.timestamp > now || now.saturating_duration_since(sample.timestamp) > self.window
        }) {
            self.presentations.pop_front();
        }
        if self.presentations.is_empty() {
            self.presentations_since = None;
        }
        if self.last_presentation.is_some_and(|last| last > now) {
            self.presentations.clear();
            self.presentations_since = None;
            self.last_presentation = None;
        }
    }

    fn complete_samples(
        &self,
        samples: &VecDeque<Sample>,
        since: Option<Instant>,
        now: Instant,
    ) -> bool {
        since.zip(samples.back()).is_some_and(|(first, last)| {
            last.timestamp <= now
                && now.saturating_duration_since(last.timestamp) <= self.window
                && now.saturating_duration_since(first) >= self.window
        })
    }

    fn complete_presentations(&self, now: Instant) -> bool {
        self.presentations_since
            .zip(self.presentations.back())
            .is_some_and(|(first, last)| {
                last.timestamp <= now
                    && now.saturating_duration_since(last.timestamp) <= self.window
                    && now.saturating_duration_since(first) >= self.window
            })
    }

    fn average(samples: &VecDeque<Sample>) -> f32 {
        (samples
            .iter()
            .map(|sample| sample.value as f64)
            .sum::<f64>()
            / samples.len() as f64) as f32
    }

    fn jitter_ms(&self) -> f32 {
        let count = self.presentations.len() as f64;
        let mean = self
            .presentations
            .iter()
            .map(|sample| sample.frametime_ms as f64)
            .sum::<f64>()
            / count;
        let variance = self
            .presentations
            .iter()
            .map(|sample| (sample.frametime_ms as f64 - mean).powi(2))
            .sum::<f64>()
            / count;
        variance.sqrt() as f32
    }

    fn low_10_fps(&self) -> f32 {
        let mut values = self
            .presentations
            .iter()
            .map(|sample| sample.fps)
            .collect::<Vec<_>>();
        values.sort_by(f32::total_cmp);
        let index = ((values.len() as f64 * 0.10).ceil() as usize)
            .max(1)
            .min(values.len())
            - 1;
        values[index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(timestamp: Instant, value: f32, status: TelemetryStatus) -> GpuMeasurement {
        GpuMeasurement {
            value,
            timestamp,
            status,
            device_id: Some(0),
        }
    }

    fn hardware(vrm: Option<f32>, fan: Option<f32>) -> HardwareMetrics {
        HardwareMetrics {
            gpu_vrm_temp_c: vrm,
            fan_rpm: fan,
            ..HardwareMetrics::default()
        }
    }

    fn record(history: &mut AutoMetricsHistory, at: Instant, gpu_value: f32, vrm: f32, fan: f32) {
        history.record(
            at,
            &gpu(at, gpu_value, TelemetryStatus::Available),
            &hardware(Some(vrm), Some(fan)),
            at,
        );
    }

    #[test]
    fn waits_for_a_complete_duration_window() {
        let start = Instant::now();
        let mut history = AutoMetricsHistory::new(Duration::from_secs(2));
        for second in 0..=2 {
            let at = start + Duration::from_secs(second);
            record(&mut history, at, 50.0, 60.0, 1200.0);
            assert_eq!(
                history.snapshot(at, AutoMetric::GpuVrmTempC).is_some(),
                second == 2
            );
        }
        let snapshot = history
            .snapshot(start + Duration::from_secs(2), AutoMetric::GpuVrmTempC)
            .unwrap();
        assert_eq!(snapshot.hardware_metrics().gpu_vrm_temp_c, Some(60.0));
    }

    #[test]
    fn full_window_stays_ready_after_oldest_sample_is_evicted() {
        let start = Instant::now();
        let mut history = AutoMetricsHistory::new(Duration::from_millis(500));
        for millis in [0, 100, 300, 400, 600, 700, 900, 1000, 1100] {
            let at = start + Duration::from_millis(millis);
            record(&mut history, at, 50.0, 80.0, 2000.0);
            if millis >= 700 {
                let snapshot = history.snapshot(at, AutoMetric::Fps).unwrap();
                if millis == 1100 {
                    assert!((snapshot.fps() - 8.0).abs() < 0.001);
                }
            }
        }
    }

    #[test]
    fn inactive_histories_evict_old_samples_without_snapshots() {
        let start = Instant::now();
        let mut history = AutoMetricsHistory::new(Duration::from_secs(1));
        for tick in 0..=100 {
            let at = start + Duration::from_millis(tick * 100);
            record(&mut history, at, 50.0, 80.0, 2000.0);
        }
        assert!(history.gpu.len() <= 11);
        assert!(history.vrm.len() <= 11);
        assert!(history.fan.len() <= 11);
        assert!(history.presentations.len() <= 11);
        assert!(history
            .snapshot(start + Duration::from_millis(10_000), AutoMetric::Fps)
            .is_some());
    }

    #[test]
    fn keeps_rolling_samples_across_many_presentations() {
        let start = Instant::now();
        let mut history = AutoMetricsHistory::new(Duration::from_secs(1));
        for tick in 0..=20 {
            let at = start + Duration::from_millis(tick * 100);
            record(
                &mut history,
                at,
                40.0 + tick as f32,
                50.0 + tick as f32,
                tick as f32,
            );
        }
        let snapshot = history
            .snapshot(start + Duration::from_secs(2), AutoMetric::Fps)
            .unwrap();
        assert!(snapshot.fps() > 0.0);
        assert!(snapshot.jitter_ms() >= 0.0);
        assert!(snapshot.low_10_fps() > 0.0);
        assert!((snapshot.gpu_measurement().value - 55.0).abs() < 0.001);
        assert_eq!(snapshot.hardware_metrics().gpu_vrm_temp_c, Some(65.0));
        assert_eq!(snapshot.hardware_metrics().fan_rpm, Some(15.0));
    }

    #[test]
    fn stale_samples_and_future_samples_are_not_reused() {
        let start = Instant::now();
        let mut history = AutoMetricsHistory::new(Duration::from_secs(1));
        for second in 0..=1 {
            record(
                &mut history,
                start + Duration::from_secs(second),
                40.0,
                50.0,
                1000.0,
            );
        }
        assert!(history
            .snapshot(start + Duration::from_secs(3), AutoMetric::FanRpm)
            .is_none());
        let future = start + Duration::from_secs(5);
        history.record(
            start + Duration::from_secs(4),
            &gpu(future, 50.0, TelemetryStatus::Available),
            &hardware(Some(50.0), Some(1000.0)),
            future,
        );
        assert!(history
            .snapshot(start + Duration::from_secs(4), AutoMetric::Fps)
            .is_none());
    }

    #[test]
    fn invalid_or_missing_values_restart_only_their_metric_window() {
        let start = Instant::now();
        let mut history = AutoMetricsHistory::new(Duration::from_secs(1));
        for second in 0..=1 {
            record(
                &mut history,
                start + Duration::from_secs(second),
                40.0,
                50.0,
                1000.0,
            );
        }
        let at = start + Duration::from_secs(2);
        history.record(
            at,
            &gpu(at, 101.0, TelemetryStatus::Available),
            &hardware(Some(f32::NAN), Some(-1.0)),
            at,
        );
        assert!(history.snapshot(at, AutoMetric::Fps).is_none());
        assert!(history.snapshot(at, AutoMetric::GpuVrmTempC).is_none());
        assert!(history.snapshot(at, AutoMetric::FanRpm).is_none());

        for second in 3..=4 {
            let at = start + Duration::from_secs(second);
            record(&mut history, at, 40.0, 50.0, 1000.0);
        }
        assert!(history
            .snapshot(start + Duration::from_secs(4), AutoMetric::FanRpm)
            .is_some());
    }

    #[test]
    fn unavailable_gpu_status_prevents_fps_snapshots() {
        let start = Instant::now();
        let mut history = AutoMetricsHistory::new(Duration::from_secs(1));
        for tick in 0..=10 {
            let at = start + Duration::from_millis(tick * 100);
            history.record(
                at,
                &gpu(
                    at,
                    50.0,
                    if tick == 10 {
                        TelemetryStatus::Expired
                    } else {
                        TelemetryStatus::Available
                    },
                ),
                &hardware(Some(50.0), Some(1000.0)),
                at,
            );
        }
        let now = start + Duration::from_secs(1);
        assert!(history.snapshot(now, AutoMetric::Fps).is_none());
        assert!(history.snapshot(now, AutoMetric::GpuVrmTempC).is_some());
    }

    #[test]
    fn invalid_fan_does_not_invalidate_vrm_or_fps_history() {
        let start = Instant::now();
        let mut history = AutoMetricsHistory::new(Duration::from_secs(1));
        for tick in 0..=11 {
            let at = start + Duration::from_millis(tick * 100);
            history.record(
                at,
                &gpu(at, 50.0, TelemetryStatus::Available),
                &hardware(Some(50.0), Some(if tick == 11 { -1.0 } else { 1000.0 })),
                at,
            );
        }
        let now = start + Duration::from_millis(1100);
        assert!(history.snapshot(now, AutoMetric::Fps).is_some());
        assert!(history.snapshot(now, AutoMetric::GpuVrmTempC).is_some());
        assert!(history.snapshot(now, AutoMetric::FanRpm).is_none());
    }

    #[test]
    fn duplicate_telemetry_timestamps_do_not_bias_averages() {
        let start = Instant::now();
        let mut history = AutoMetricsHistory::new(Duration::from_secs(1));
        for tick in 0..=28 {
            let at = start + Duration::from_millis(tick * 100);
            record(
                &mut history,
                at,
                20.0 + tick as f32,
                30.0 + tick as f32,
                500.0 + tick as f32 * 10.0,
            );
        }
        for _ in 1..=10 {
            let at = start + Duration::from_millis(2800);
            history.record(
                at,
                &gpu(
                    start + Duration::from_millis(2800),
                    90.0,
                    TelemetryStatus::Available,
                ),
                &hardware(Some(90.0), Some(1100.0)),
                start + Duration::from_millis(2800),
            );
        }
        let snapshot = history
            .snapshot(start + Duration::from_millis(2800), AutoMetric::Fps)
            .unwrap();
        assert_eq!(snapshot.gpu_measurement().value, 43.0);
        assert_eq!(snapshot.hardware_metrics().gpu_vrm_temp_c, Some(53.0));
        assert_eq!(snapshot.hardware_metrics().fan_rpm, Some(730.0));
    }

    #[test]
    fn each_history_has_an_independent_window() {
        let start = Instant::now();
        let mut short = AutoMetricsHistory::new(Duration::from_secs(1));
        let mut long = AutoMetricsHistory::new(Duration::from_secs(2));
        for second in 0..=1 {
            let at = start + Duration::from_secs(second);
            record(&mut short, at, 30.0, 40.0, 500.0);
            record(&mut long, at, 30.0, 40.0, 500.0);
        }
        let now = start + Duration::from_secs(1);
        assert!(short.snapshot(now, AutoMetric::FanRpm).is_some());
        assert!(long.snapshot(now, AutoMetric::FanRpm).is_none());
    }
}
