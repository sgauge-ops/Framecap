use crate::layer::config::{AutoAlgorithmConfig, AutoMetric, AutoTarget};
use crate::layer::lib::{MetricsSnapshot, TelemetryStatus};
use crate::layer::limiter::FrameCapController;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

struct AlgorithmState {
    config: AutoAlgorithmConfig,
    last_increase: Option<Instant>,
    last_decrease: Option<Instant>,
}

pub struct AutoRegulator {
    algorithms: Vec<AlgorithmState>,
    gpu_increase_limit: f32,
    gpu_decrease_limit: f32,
    frame_jitter_limit_ms: f32,
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
                    last_increase: None,
                    last_decrease: None,
                })
                .collect(),
            gpu_increase_limit,
            gpu_decrease_limit,
            frame_jitter_limit_ms,
        }
    }

    /// Evaluates one selected algorithm and applies an eligible target change.
    pub fn evaluate(
        &mut self,
        algorithm_index: usize,
        metrics: &MetricsSnapshot,
        now: Instant,
        hysteresis_percent: f32,
        frame_cap: &FrameCapController,
    ) -> Option<(f32, f32)> {
        if !hysteresis_percent.is_finite() || !(0.0..=100.0).contains(&hysteresis_percent) {
            return None;
        }
        let state = self.algorithms.get_mut(algorithm_index)?;
        let current_target = frame_cap.target_fps()?;
        let hysteresis = hysteresis_percent / 100.0;
        let desired_target = match (state.config.metric, state.config.target) {
            (AutoMetric::Fps, AutoTarget::Low10Fps) => {
                let gpu = metrics.gpu_measurement();
                if gpu.status != TelemetryStatus::Available
                    || !gpu.value.is_finite()
                    || !(0.0..=100.0).contains(&gpu.value)
                    || gpu.timestamp > now
                    || now.saturating_duration_since(gpu.timestamp)
                        > state.config.measurement_expiry
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
                let margin_fps = current_target * hysteresis;
                if gpu.value < self.gpu_increase_limit && current_target - low_10_fps <= margin_fps
                {
                    (current_target + state.config.increase_step_fps).floor()
                } else if low_10_fps < observed_fps - margin_fps
                    && (gpu.value >= self.gpu_decrease_limit
                        || metrics.jitter_ms() > self.frame_jitter_limit_ms)
                {
                    (current_target - state.config.decrease_step_fps).floor()
                } else {
                    return None;
                }
            }
            (AutoMetric::GpuVrmTempC, AutoTarget::Value(target_c)) => {
                if now < metrics.hardware_timestamp()
                    || now.saturating_duration_since(metrics.hardware_timestamp())
                        > state.config.measurement_expiry
                {
                    return None;
                }
                let temperature = metrics.hardware_metrics().gpu_vrm_temp_c?;
                if !temperature.is_finite() || !target_c.is_finite() || target_c <= 0.0 {
                    return None;
                }
                let band = target_c * hysteresis;
                if temperature < target_c - band {
                    (current_target + state.config.increase_step_fps).floor()
                } else if temperature > target_c + band {
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

        let (last_adjustment, interval) = if delta > 0.0 {
            (&mut state.last_increase, state.config.increase_interval)
        } else {
            (&mut state.last_decrease, state.config.decrease_interval)
        };
        if last_adjustment.is_some_and(|last| now.saturating_duration_since(last) < interval) {
            return None;
        }
        let minimum = state.config.min_fps.ceil();
        let maximum = state.config.max_fps.floor();
        if minimum > maximum {
            return None;
        }
        let adjustment = frame_cap.adjust_with_bounds(delta, minimum, maximum)?;
        *last_adjustment = Some(now);
        Some(adjustment)
    }
}

/// Atomically changes the active algorithm's hysteresis in percentage points.
pub fn adjust_hysteresis_percent(
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
        if !current.is_finite() || !(0.0..=100.0).contains(&current) {
            return None;
        }
        let next = (current + delta).clamp(0.0, 100.0);
        if next == current {
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

    fn algorithm(metric: AutoMetric, target: AutoTarget, expiry: Duration) -> AutoAlgorithmConfig {
        AutoAlgorithmConfig {
            name: "test".to_owned(),
            metric,
            target,
            hysteresis_percent: 5.0,
            increase_interval: Duration::ZERO,
            decrease_interval: Duration::ZERO,
            increase_step_fps: 5.0,
            decrease_step_fps: 5.0,
            min_fps: 1.0,
            max_fps: MAX_TARGET_FPS,
            measurement_expiry: expiry,
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
        first.increase_interval = Duration::from_secs(1);
        let second = algorithm(
            AutoMetric::GpuVrmTempC,
            AutoTarget::Value(100.0),
            Duration::from_secs(1),
        );
        let mut regulator = AutoRegulator::new(vec![first, second], 98.0, 90.0, 2.0);
        let cap = FrameCapController::new(Some(60.0));
        let snapshot = metrics(now, 60.0, 55.0, 50.0, Some(94.0));
        assert_eq!(
            regulator.evaluate(0, &snapshot, now, 5.0, &cap),
            Some((60.0, 65.0))
        );
        assert_eq!(
            regulator.evaluate(1, &snapshot, now, 5.0, &cap),
            Some((65.0, 70.0))
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
    fn hysteresis_hotkeys_change_percentage_within_bounds() {
        let values = [AtomicU32::new(5.0f32.to_bits())];
        assert_eq!(
            adjust_hysteresis_percent(&values, 0, -1.0),
            Some((5.0, 4.0))
        );
        assert_eq!(adjust_hysteresis_percent(&values, 0, 1.0), Some((4.0, 5.0)));
        for _ in 0..100 {
            adjust_hysteresis_percent(&values, 0, 1.0);
        }
        assert_eq!(f32::from_bits(values[0].load(Ordering::Relaxed)), 100.0);
        assert_eq!(adjust_hysteresis_percent(&values, 0, 1.0), None);
    }
}
