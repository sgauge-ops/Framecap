use crate::layer::config::{AutoAlgorithmConfig, AutoMetric, AutoTarget};
use crate::layer::lib::{MetricsSnapshot, TelemetryStatus};
use crate::layer::limiter::FrameCapController;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

struct AlgorithmState {
    config: AutoAlgorithmConfig,
    pending: Option<(bool, Instant)>,
    last_adjustment: Option<Instant>,
    last_cap_generation: Option<u64>,
    last_hysteresis: Option<f32>,
}

pub struct AutoRegulator {
    algorithms: Vec<AlgorithmState>,
    gpu_increase_limit: f32,
    gpu_decrease_limit: f32,
    frame_jitter_limit_ms: f32,
    active_algorithm: Option<usize>,
}

pub struct AutoEvaluationSchedule {
    interval: Duration,
    last_evaluation: Option<Instant>,
    active_algorithm: Option<usize>,
}

impl AutoEvaluationSchedule {
    /// Creates a schedule that permits one evaluation immediately.
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last_evaluation: None,
            active_algorithm: None,
        }
    }

    /// Returns whether an evaluation is due and records its scheduled time.
    pub fn should_evaluate(&mut self, now: Instant, active_algorithm: Option<usize>) -> bool {
        let Some(active_algorithm) = active_algorithm else {
            self.last_evaluation = None;
            self.active_algorithm = None;
            return false;
        };
        if self.active_algorithm != Some(active_algorithm) {
            self.active_algorithm = Some(active_algorithm);
            self.last_evaluation = Some(now);
            return true;
        }
        if self
            .last_evaluation
            .is_some_and(|last| now.saturating_duration_since(last) < self.interval)
        {
            return false;
        }
        self.last_evaluation = Some(now);
        true
    }

    pub fn has_gap(&self, now: Instant) -> bool {
        let tolerance = self.interval.saturating_mul(2).max(Duration::from_secs(2));
        self.last_evaluation
            .is_some_and(|last| now.saturating_duration_since(last) > tolerance)
    }
}

impl AutoRegulator {
    /// Creates independent runtime state for the configured algorithms.
    pub fn new(
        algorithms: Vec<AutoAlgorithmConfig>,
        gpu_increase_limit: f32,
        gpu_decrease_limit: f32,
        frame_jitter_limit_ms: f32,
    ) -> Self {
        Self {
            algorithms: algorithms
                .into_iter()
                .map(|config| AlgorithmState {
                    config,
                    pending: None,
                    last_adjustment: None,
                    last_cap_generation: None,
                    last_hysteresis: None,
                })
                .collect(),
            gpu_increase_limit,
            gpu_decrease_limit,
            frame_jitter_limit_ms,
            active_algorithm: None,
        }
    }

    pub fn set_active(&mut self, algorithm_index: Option<usize>) {
        if self.active_algorithm != algorithm_index {
            for state in &mut self.algorithms {
                state.pending = None;
            }
            self.active_algorithm = algorithm_index;
        }
    }

    pub fn reset_pending(&mut self, algorithm_index: usize) {
        if let Some(state) = self.algorithms.get_mut(algorithm_index) {
            state.pending = None;
        }
    }

    pub fn windows(&self) -> impl Iterator<Item = Duration> + '_ {
        self.algorithms.iter().map(|state| state.config.window)
    }

    pub fn metric(&self, algorithm_index: usize) -> Option<AutoMetric> {
        self.algorithms
            .get(algorithm_index)
            .map(|state| state.config.metric)
    }

    /// Returns the overlay label for a configured algorithm.
    pub fn overlay_label(&self, algorithm_index: usize) -> Option<&'static str> {
        match self.algorithms.get(algorithm_index)?.config.name.as_str() {
            "gpu_threshold" => Some("JIT"),
            "vrm_temp" => Some("VRM"),
            "fan_rpm" => Some("FAN"),
            _ => None,
        }
    }

    pub fn overlay_target(&self, algorithm_index: usize) -> Option<(AutoMetric, AutoTarget)> {
        let config = &self.algorithms.get(algorithm_index)?.config;
        Some((config.metric, config.target))
    }

    /// Evaluates one selected algorithm and applies an eligible target change.
    pub fn evaluate(
        &mut self,
        algorithm_index: usize,
        metrics: &MetricsSnapshot,
        now: Instant,
        hysteresis: f32,
        frame_cap: &FrameCapController,
    ) -> Option<(f32, f32)> {
        let state = self.algorithms.get_mut(algorithm_index)?;
        let generation = frame_cap.generation();
        if state.last_cap_generation != Some(generation)
            || state.last_hysteresis != Some(hysteresis)
        {
            state.pending = None;
        }
        state.last_cap_generation = Some(generation);
        state.last_hysteresis = Some(hysteresis);
        let desired_target = (|| {
            if !hysteresis.is_finite() || hysteresis < 0.0 {
                return None;
            }
            let current_target = frame_cap.target_fps()?;
            let desired_target = match (state.config.metric, state.config.target) {
                (AutoMetric::Fps, AutoTarget::Low10Fps) => {
                    let gpu = metrics.gpu_measurement();
                    if gpu.status != TelemetryStatus::Available
                        || !gpu.value.is_finite()
                        || !(0.0..=100.0).contains(&gpu.value)
                        || gpu.timestamp > now
                        || now.saturating_duration_since(gpu.timestamp) > state.config.window
                    {
                        return None;
                    }
                    let observed_fps = metrics.fps();
                    let low_10_fps = metrics.low_10_fps();
                    if !observed_fps.is_finite()
                        || observed_fps <= 0.0
                        || !low_10_fps.is_finite()
                        || low_10_fps <= 0.0
                    {
                        return None;
                    }
                    let margin_fps = hysteresis;
                    if (gpu.value < self.gpu_increase_limit)
                        && ((current_target - low_10_fps) <= margin_fps)
                    {
                        (current_target + state.config.increase_step_fps).floor()
                    } else if (low_10_fps < (observed_fps - margin_fps))
                        && ((gpu.value >= self.gpu_decrease_limit)
                            || (metrics.jitter_ms() > self.frame_jitter_limit_ms))
                    {
                        let decreased_target =
                            (current_target - state.config.decrease_step_fps).floor();
                        if decreased_target < low_10_fps {
                            return None;
                        }
                        decreased_target
                    } else {
                        return None;
                    }
                }
                (AutoMetric::GpuVrmTempC, AutoTarget::Value(target_c)) => {
                    if now < metrics.hardware_timestamp()
                        || now.saturating_duration_since(metrics.hardware_timestamp())
                            > state.config.window
                    {
                        return None;
                    }
                    let temperature = metrics.hardware_metrics().gpu_vrm_temp_c?;
                    if !temperature.is_finite() || !target_c.is_finite() || target_c <= 0.0 {
                        return None;
                    }
                    let band = hysteresis;
                    if temperature < target_c - band {
                        (current_target + state.config.increase_step_fps).floor()
                    } else if temperature > target_c + band {
                        (current_target - state.config.decrease_step_fps).floor()
                    } else {
                        return None;
                    }
                }
                (AutoMetric::FanRpm, AutoTarget::Value(target_rpm)) => {
                    if now < metrics.hardware_timestamp()
                        || now.saturating_duration_since(metrics.hardware_timestamp())
                            > state.config.window
                    {
                        return None;
                    }
                    let fan_rpm = metrics.hardware_metrics().fan_rpm?;
                    if !fan_rpm.is_finite()
                        || fan_rpm < 0.0
                        || !target_rpm.is_finite()
                        || target_rpm <= 0.0
                    {
                        return None;
                    }
                    let band = hysteresis;
                    if fan_rpm < target_rpm - band {
                        (current_target + state.config.increase_step_fps).floor()
                    } else if fan_rpm > target_rpm + band {
                        (current_target - state.config.decrease_step_fps).floor()
                    } else {
                        return None;
                    }
                }
                _ => return None,
            };
            if !desired_target.is_finite() {
                return None;
            }
            let delta = desired_target - current_target;
            if delta == 0.0 {
                return None;
            }

            let minimum = state.config.min_fps.ceil();
            let maximum = state.config.max_fps.floor();
            if minimum > maximum {
                return None;
            }
            Some((delta, minimum, maximum))
        })();
        let Some((delta, minimum, maximum)) = desired_target else {
            state.pending = None;
            return None;
        };
        let direction = delta > 0.0;
        let since = match state.pending {
            Some((previous, since)) if previous == direction && now >= since => since,
            _ => {
                state.pending = Some((direction, now));
                now
            }
        };
        if now.saturating_duration_since(since) < state.config.window
            || state
                .last_adjustment
                .is_some_and(|last| now.saturating_duration_since(last) < state.config.window)
        {
            return None;
        }
        let adjustment = frame_cap.adjust_with_bounds(delta, minimum, maximum)?;
        state.last_adjustment = Some(now);
        state.last_cap_generation = Some(frame_cap.generation());
        state.pending = None;
        Some(adjustment)
    }
}

/// Atomically changes the active algorithm's hysteresis in its metric's units.
pub fn adjust_hysteresis(
    hysteresis: &[AtomicU32],
    algorithm_index: usize,
    delta: f32,
) -> Option<(f32, f32)> {
    if !delta.is_finite() || delta == 0.0 {
        return None;
    }
    let value = hysteresis.get(algorithm_index)?;
    let mut current_bits = value.load(Ordering::Relaxed);
    loop {
        let current = f32::from_bits(current_bits);
        if !current.is_finite() || current < 0.0 {
            return None;
        }
        let next = (current + delta).max(0.0);
        if !next.is_finite() || next == current {
            return None;
        }
        match value.compare_exchange_weak(
            current_bits,
            next.to_bits(),
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return Some((current, next)),
            Err(actual) => current_bits = actual,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::config::MAX_TARGET_FPS;
    use crate::layer::lib::{GpuMeasurement, HardwareMetrics};

    fn algorithm(metric: AutoMetric, target: AutoTarget, _expiry: Duration) -> AutoAlgorithmConfig {
        AutoAlgorithmConfig {
            name: "test".to_owned(),
            metric,
            target,
            hysteresis: 5.0,
            window: Duration::ZERO,
            increase_step_fps: 5.0,
            decrease_step_fps: 5.0,
            min_fps: 1.0,
            max_fps: MAX_TARGET_FPS,
        }
    }

    fn metrics(
        now: Instant,
        fps: f32,
        low_10_fps: f32,
        gpu: f32,
        vrm_temp: Option<f32>,
    ) -> MetricsSnapshot {
        MetricsSnapshot::new_with_hardware_metrics_at(
            1000.0 / fps,
            fps,
            0.0,
            low_10_fps,
            GpuMeasurement {
                value: gpu,
                timestamp: now,
                status: TelemetryStatus::Available,
                device_id: None,
            },
            HardwareMetrics {
                gpu_vrm_temp_c: vrm_temp,
                ..HardwareMetrics::default()
            },
            now,
            now,
        )
    }

    fn fan_metrics(now: Instant, fan_rpm: Option<f32>) -> MetricsSnapshot {
        MetricsSnapshot::new_with_hardware_metrics_at(
            1000.0 / 60.0,
            60.0,
            0.0,
            55.0,
            GpuMeasurement {
                value: 50.0,
                timestamp: now,
                status: TelemetryStatus::Available,
                device_id: None,
            },
            HardwareMetrics {
                fan_rpm,
                ..HardwareMetrics::default()
            },
            now,
            now,
        )
    }

    #[test]
    fn evaluation_schedule_observes_interval_and_auto_mode() {
        let now = Instant::now();
        let mut schedule = AutoEvaluationSchedule::new(Duration::from_millis(100));
        assert!(schedule.should_evaluate(now, Some(0)));
        assert!(!schedule.should_evaluate(now + Duration::from_millis(99), Some(0)));
        assert!(schedule.should_evaluate(now + Duration::from_millis(100), Some(0)));
        assert!(!schedule.should_evaluate(now, None));
        assert!(schedule.should_evaluate(now, Some(1)));
    }

    #[test]
    fn evaluation_schedule_reports_long_interruption() {
        let now = Instant::now();
        let mut schedule = AutoEvaluationSchedule::new(Duration::from_millis(100));
        assert!(schedule.should_evaluate(now, Some(0)));
        assert!(!schedule.has_gap(now + Duration::from_millis(2000)));
        assert!(schedule.has_gap(now + Duration::from_millis(2001)));
    }

    #[test]
    fn overlay_labels_follow_algorithm_names_not_their_order() {
        let mut fan = algorithm(
            AutoMetric::FanRpm,
            AutoTarget::Value(2000.0),
            Duration::from_secs(1),
        );
        fan.name = "fan_rpm".to_owned();
        let mut jitter = algorithm(
            AutoMetric::Fps,
            AutoTarget::Low10Fps,
            Duration::from_secs(1),
        );
        jitter.name = "gpu_threshold".to_owned();
        let mut vrm = algorithm(
            AutoMetric::GpuVrmTempC,
            AutoTarget::Value(100.0),
            Duration::from_secs(1),
        );
        vrm.name = "vrm_temp".to_owned();
        let regulator = AutoRegulator::new(vec![fan, jitter, vrm], 98.0, 90.0, 2.0);

        assert_eq!(regulator.overlay_label(0), Some("FAN"));
        assert_eq!(regulator.overlay_label(1), Some("JIT"));
        assert_eq!(regulator.overlay_label(2), Some("VRM"));
        assert_eq!(
            regulator.overlay_target(0),
            Some((AutoMetric::FanRpm, AutoTarget::Value(2000.0)))
        );
        assert_eq!(
            regulator.overlay_target(1),
            Some((AutoMetric::Fps, AutoTarget::Low10Fps))
        );
        assert_eq!(
            regulator.overlay_target(2),
            Some((AutoMetric::GpuVrmTempC, AutoTarget::Value(100.0)))
        );
        assert_eq!(regulator.overlay_target(3), None);
    }

    #[test]
    fn gpu_threshold_uses_low_ten_hysteresis_and_gpu_gates() {
        let now = Instant::now();
        let mut regulator = AutoRegulator::new(
            vec![algorithm(
                AutoMetric::Fps,
                AutoTarget::Low10Fps,
                Duration::from_secs(1),
            )],
            98.0,
            90.0,
            2.0,
        );
        let cap = FrameCapController::new(Some(60.0));
        assert_eq!(
            regulator.evaluate(0, &metrics(now, 60.0, 59.0, 50.0, None), now, 3.0, &cap),
            Some((60.0, 65.0))
        );

        let cap = FrameCapController::new(Some(60.0));
        assert_eq!(
            regulator.evaluate(0, &metrics(now, 60.0, 50.0, 99.0, None), now, 3.0, &cap),
            Some((60.0, 55.0))
        );
    }

    #[test]
    fn low_ten_fps_decrease_does_not_go_below_target() {
        let now = Instant::now();
        let mut regulator = AutoRegulator::new(
            vec![algorithm(
                AutoMetric::Fps,
                AutoTarget::Low10Fps,
                Duration::from_secs(1),
            )],
            98.0,
            90.0,
            2.0,
        );
        let cap = FrameCapController::new(Some(60.0));

        assert_eq!(
            regulator.evaluate(0, &metrics(now, 60.0, 56.0, 99.0, None), now, 3.0, &cap),
            None
        );
        assert_eq!(cap.target_fps(), Some(60.0));
    }

    #[test]
    fn fan_rpm_adjusts_around_target_and_ignores_missing_or_stale_readings() {
        let now = Instant::now();
        for fan_rpm in [1900.0, 2000.0, 2100.0] {
            let mut regulator = AutoRegulator::new(
                vec![algorithm(
                    AutoMetric::FanRpm,
                    AutoTarget::Value(2000.0),
                    Duration::from_secs(1),
                )],
                98.0,
                90.0,
                2.0,
            );
            let cap = FrameCapController::new(Some(60.0));
            assert_eq!(
                regulator.evaluate(0, &fan_metrics(now, Some(fan_rpm)), now, 100.0, &cap),
                None
            );
        }

        for (fan_rpm, expected) in [(1899.0, (60.0, 65.0)), (2101.0, (60.0, 55.0))] {
            let mut regulator = AutoRegulator::new(
                vec![algorithm(
                    AutoMetric::FanRpm,
                    AutoTarget::Value(2000.0),
                    Duration::from_secs(1),
                )],
                98.0,
                90.0,
                2.0,
            );
            let cap = FrameCapController::new(Some(60.0));
            assert_eq!(
                regulator.evaluate(0, &fan_metrics(now, Some(fan_rpm)), now, 100.0, &cap),
                Some(expected)
            );
        }

        let mut regulator = AutoRegulator::new(
            vec![algorithm(
                AutoMetric::FanRpm,
                AutoTarget::Value(2000.0),
                Duration::from_millis(500),
            )],
            98.0,
            90.0,
            2.0,
        );
        let cap = FrameCapController::new(Some(60.0));
        assert_eq!(
            regulator.evaluate(0, &fan_metrics(now, None), now, 5.0, &cap),
            None
        );
        let stale_time = now - Duration::from_secs(1);
        assert_eq!(
            regulator.evaluate(0, &fan_metrics(stale_time, Some(1000.0)), now, 5.0, &cap),
            None
        );
    }

    #[test]
    fn vrm_temperature_holds_inclusive_hysteresis_band() {
        let now = Instant::now();
        for temperature in [95.0, 100.0, 105.0] {
            let mut regulator = AutoRegulator::new(
                vec![algorithm(
                    AutoMetric::GpuVrmTempC,
                    AutoTarget::Value(100.0),
                    Duration::from_secs(1),
                )],
                98.0,
                90.0,
                2.0,
            );
            let cap = FrameCapController::new(Some(60.0));
            assert_eq!(
                regulator.evaluate(
                    0,
                    &metrics(now, 60.0, 55.0, 50.0, Some(temperature)),
                    now,
                    5.0,
                    &cap
                ),
                None
            );
        }
    }

    #[test]
    fn vrm_temperature_adjusts_outside_band_and_ignores_stale_readings() {
        let now = Instant::now();
        for (temperature, expected) in [(94.9, (60.0, 65.0)), (105.1, (60.0, 55.0))] {
            let mut regulator = AutoRegulator::new(
                vec![algorithm(
                    AutoMetric::GpuVrmTempC,
                    AutoTarget::Value(100.0),
                    Duration::from_secs(1),
                )],
                98.0,
                90.0,
                2.0,
            );
            let cap = FrameCapController::new(Some(60.0));
            assert_eq!(
                regulator.evaluate(
                    0,
                    &metrics(now, 60.0, 55.0, 50.0, Some(temperature)),
                    now,
                    5.0,
                    &cap
                ),
                Some(expected)
            );
        }
        let mut regulator = AutoRegulator::new(
            vec![algorithm(
                AutoMetric::GpuVrmTempC,
                AutoTarget::Value(100.0),
                Duration::from_millis(500),
            )],
            98.0,
            90.0,
            2.0,
        );
        let cap = FrameCapController::new(Some(60.0));
        let stale_time = now - Duration::from_secs(1);
        assert_eq!(
            regulator.evaluate(
                0,
                &metrics(stale_time, 60.0, 55.0, 50.0, Some(80.0)),
                now,
                5.0,
                &cap
            ),
            None
        );
    }

    #[test]
    fn algorithms_keep_independent_increase_intervals() {
        let now = Instant::now();
        let mut first = algorithm(
            AutoMetric::GpuVrmTempC,
            AutoTarget::Value(100.0),
            Duration::from_secs(1),
        );
        first.window = Duration::from_secs(1);
        let second = algorithm(
            AutoMetric::GpuVrmTempC,
            AutoTarget::Value(100.0),
            Duration::from_secs(1),
        );
        let mut regulator = AutoRegulator::new(vec![first, second], 98.0, 90.0, 2.0);
        let cap = FrameCapController::new(Some(60.0));
        let snapshot = metrics(now, 60.0, 55.0, 50.0, Some(94.0));
        assert_eq!(regulator.evaluate(0, &snapshot, now, 5.0, &cap), None);
        assert_eq!(
            regulator.evaluate(1, &snapshot, now, 5.0, &cap),
            Some((60.0, 65.0))
        );
        assert_eq!(
            regulator.evaluate(
                0,
                &metrics(
                    now + Duration::from_millis(100),
                    60.0,
                    55.0,
                    50.0,
                    Some(94.0)
                ),
                now + Duration::from_millis(100),
                5.0,
                &cap
            ),
            None
        );
    }

    #[test]
    fn persistent_direction_requires_full_window_after_neutral_and_each_change() {
        let now = Instant::now();
        let mut config = algorithm(
            AutoMetric::GpuVrmTempC,
            AutoTarget::Value(100.0),
            Duration::ZERO,
        );
        config.window = Duration::from_secs(4);
        let mut regulator = AutoRegulator::new(vec![config], 98.0, 90.0, 2.0);
        let cap = FrameCapController::new(Some(60.0));
        let check = |regulator: &mut AutoRegulator, seconds, temp| {
            let time = now + Duration::from_secs(seconds);
            regulator.evaluate(
                0,
                &metrics(time, 60.0, 55.0, 50.0, Some(temp)),
                time,
                5.0,
                &cap,
            )
        };
        assert_eq!(check(&mut regulator, 0, 94.0), None);
        assert_eq!(check(&mut regulator, 3, 94.0), None);
        assert_eq!(check(&mut regulator, 4, 100.0), None);
        assert_eq!(check(&mut regulator, 5, 94.0), None);
        assert_eq!(check(&mut regulator, 8, 94.0), None);
        assert_eq!(check(&mut regulator, 9, 94.0), Some((60.0, 65.0)));
        assert_eq!(check(&mut regulator, 10, 106.0), None);
        assert_eq!(check(&mut regulator, 13, 106.0), None);
        assert_eq!(check(&mut regulator, 14, 106.0), Some((65.0, 60.0)));
    }

    #[test]
    fn direction_change_missing_data_and_mode_change_reset_confirmation() {
        let now = Instant::now();
        let mut config = algorithm(
            AutoMetric::GpuVrmTempC,
            AutoTarget::Value(100.0),
            Duration::ZERO,
        );
        config.window = Duration::from_secs(4);
        let mut regulator = AutoRegulator::new(vec![config], 98.0, 90.0, 2.0);
        let cap = FrameCapController::new(Some(60.0));
        regulator.set_active(Some(0));
        for (seconds, temp) in [
            (0, Some(94.0)),
            (3, Some(106.0)),
            (6, None),
            (7, Some(106.0)),
        ] {
            let time = now + Duration::from_secs(seconds);
            assert_eq!(
                regulator.evaluate(0, &metrics(time, 60.0, 55.0, 50.0, temp), time, 5.0, &cap),
                None
            );
        }
        regulator.set_active(None);
        regulator.set_active(Some(0));
        let time = now + Duration::from_secs(11);
        assert_eq!(
            regulator.evaluate(
                0,
                &metrics(time, 60.0, 55.0, 50.0, Some(106.0)),
                time,
                5.0,
                &cap
            ),
            None
        );
        let time = now + Duration::from_secs(15);
        assert_eq!(
            regulator.evaluate(
                0,
                &metrics(time, 60.0, 55.0, 50.0, Some(106.0)),
                time,
                5.0,
                &cap
            ),
            Some((60.0, 55.0))
        );
    }

    #[test]
    fn gpu_threshold_retains_priority_and_absolute_margin_when_confirming() {
        let now = Instant::now();
        let mut config = algorithm(AutoMetric::Fps, AutoTarget::Low10Fps, Duration::ZERO);
        config.window = Duration::from_secs(4);
        let mut regulator = AutoRegulator::new(vec![config], 98.0, 90.0, 2.0);
        let cap = FrameCapController::new(Some(60.0));
        let check = |regulator: &mut AutoRegulator, seconds, low, gpu| {
            let time = now + Duration::from_secs(seconds);
            regulator.evaluate(0, &metrics(time, 60.0, low, gpu, None), time, 5.0, &cap)
        };
        assert_eq!(check(&mut regulator, 0, 55.0, 95.0), None);
        assert_eq!(check(&mut regulator, 4, 55.0, 95.0), Some((60.0, 65.0)));
        assert_eq!(check(&mut regulator, 5, 54.0, 99.0), None);
        assert_eq!(check(&mut regulator, 9, 54.0, 99.0), Some((65.0, 60.0)));
        assert_eq!(check(&mut regulator, 10, 56.0, 99.0), None);
        assert_eq!(check(&mut regulator, 14, 56.0, 99.0), None);
    }

    #[test]
    fn changing_hysteresis_or_cap_restarts_confirmation() {
        let now = Instant::now();
        let mut config = algorithm(
            AutoMetric::GpuVrmTempC,
            AutoTarget::Value(100.0),
            Duration::ZERO,
        );
        config.window = Duration::from_secs(4);
        let mut regulator = AutoRegulator::new(vec![config], 98.0, 90.0, 2.0);
        let cap = FrameCapController::new(Some(60.0));
        let check = |regulator: &mut AutoRegulator, seconds, hysteresis| {
            let time = now + Duration::from_secs(seconds);
            regulator.evaluate(
                0,
                &metrics(time, 60.0, 55.0, 50.0, Some(90.0)),
                time,
                hysteresis,
                &cap,
            )
        };
        assert_eq!(check(&mut regulator, 0, 5.0), None);
        assert_eq!(check(&mut regulator, 4, 6.0), None);
        assert!(cap.set_target_fps(Some(65.0)));
        assert_eq!(check(&mut regulator, 8, 6.0), None);
        assert_eq!(check(&mut regulator, 12, 6.0), Some((65.0, 70.0)));
    }

    #[test]
    fn full_window_then_persistent_direction_drives_regulation() {
        use crate::layer::auto_metrics::AutoMetricsHistory;

        let now = Instant::now();
        let mut config = algorithm(
            AutoMetric::GpuVrmTempC,
            AutoTarget::Value(100.0),
            Duration::ZERO,
        );
        config.window = Duration::from_secs(4);
        let mut history = AutoMetricsHistory::new(config.window);
        let mut regulator = AutoRegulator::new(vec![config], 98.0, 90.0, 2.0);
        let cap = FrameCapController::new(Some(60.0));

        for second in 0..=8 {
            let time = now + Duration::from_secs(second);
            let gpu = GpuMeasurement {
                value: 50.0,
                timestamp: time,
                status: TelemetryStatus::Available,
                device_id: None,
            };
            let hardware = HardwareMetrics {
                gpu_vrm_temp_c: Some(110.0),
                ..HardwareMetrics::default()
            };
            history.record(time, &gpu, &hardware, time);
            let result = history
                .snapshot(time, AutoMetric::GpuVrmTempC)
                .and_then(|snapshot| regulator.evaluate(0, &snapshot, time, 5.0, &cap));
            if second == 8 {
                assert_eq!(result, Some((60.0, 55.0)));
            } else {
                assert_eq!(result, None);
            }
        }
    }

    #[test]
    fn hysteresis_hotkeys_change_absolute_value_without_upper_percentage_limit() {
        let values = [AtomicU32::new(5.0f32.to_bits())];
        assert_eq!(adjust_hysteresis(&values, 0, -1.0), Some((5.0, 4.0)));
        assert_eq!(adjust_hysteresis(&values, 0, 1.0), Some((4.0, 5.0)));
        for _ in 0..100 {
            adjust_hysteresis(&values, 0, 1.0);
        }
        assert_eq!(f32::from_bits(values[0].load(Ordering::Relaxed)), 105.0);
        assert_eq!(adjust_hysteresis(&values, 0, -1000.0), Some((105.0, 0.0)));
        assert_eq!(adjust_hysteresis(&values, 0, -1.0), None);
    }
}
