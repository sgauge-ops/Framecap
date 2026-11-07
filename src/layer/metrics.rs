use crate::layer::lib::MetricsSnapshot;
use crate::layer::telemetry::GpuTelemetryProvider;

use std::time::{Duration, Instant};

pub struct MetricsAggregator {
    samples: Vec<Duration>,
    window_size: usize,
    telemetry_provider: Box<dyn GpuTelemetryProvider>,
}

impl MetricsAggregator {
    /// Creates an empty count-based metrics window backed by the given GPU provider.
    pub fn new(window_size: usize, provider: Box<dyn GpuTelemetryProvider>) -> Self {
        Self {
            samples: Vec::with_capacity(window_size),
            window_size,
            telemetry_provider: provider,
        }
    }

    /// Records a presentation interval and returns the resulting metrics snapshot.
    pub fn add_sample(&mut self, interval: Duration) -> MetricsSnapshot {
        if self.samples.len() >= self.window_size {
            self.samples.remove(0);
        }
        self.samples.push(interval);

        let frametime = (interval.as_secs_f64() * 1000.0) as f32;
        let jitter = if self.samples.len() < self.window_size {
            -1.0 // Valeur sentinelle pour "collecte..."
        } else {
            self.calculate_jitter() as f32
        };
        let low_10_fps = if self.samples.len() < self.window_size {
            -1.0
        } else {
            self.calculate_low_10_fps() as f32
        };

        let gpu_measurement = self.telemetry_provider.get_metric();

        MetricsSnapshot::new(
            frametime,
            jitter,
            low_10_fps,
            gpu_measurement,
            Instant::now(),
        )
    }

    /// Calculates the population standard deviation of the buffered frametimes.
    fn calculate_jitter(&self) -> f64 {
        let count = self.samples.len() as f64;
        let mean = self
            .samples
            .iter()
            .map(|s| s.as_secs_f64() * 1000.0)
            .sum::<f64>()
            / count;

        let variance = self
            .samples
            .iter()
            .map(|s| {
                let val = s.as_secs_f64() * 1000.0;
                (val - mean).powi(2)
            })
            .sum::<f64>()
            / count;

        variance.sqrt()
    }

    /// Returns the tenth-percentile FPS from the buffered presentation intervals.
    fn calculate_low_10_fps(&self) -> f64 {
        let mut fps_values = self
            .samples
            .iter()
            .map(|sample| 1.0 / sample.as_secs_f64())
            .collect::<Vec<_>>();
        fps_values.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let index = ((fps_values.len() as f64 * 0.10).ceil() as usize)
            .max(1)
            .min(fps_values.len())
            .saturating_sub(1);
        fps_values[index]
    }

    /// Clears the accumulated frametime samples.
    pub fn reset(&mut self) {
        self.samples.clear();
    }
}
