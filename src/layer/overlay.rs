use crate::layer::lib::{MetricsSnapshot, TelemetryStatus};

pub struct Overlay {
    pub color: [u8; 4],
}

impl Overlay {
    /// Creates a text overlay with opaque white text.
    pub fn new() -> Self {
        Self {
            color: [255, 255, 255, 255],
        }
    }

    /// Prints a readable diagnostic representation of the current metrics.
    pub fn render(&self, metrics: &MetricsSnapshot) {
        let jitter_ms = metrics.jitter_ms();
        let gpu_measurement = metrics.gpu_measurement();
        let jitter_str = if jitter_ms < 0.0 {
            "collecte...".to_string()
        } else {
            format!("{jitter_ms:.2} ms")
        };

        let gpu_str = match gpu_measurement.status {
            TelemetryStatus::Available => format!("{}%", gpu_measurement.value),
            TelemetryStatus::Expired => "périmée".to_string(),
            TelemetryStatus::Unsupported => "non pris en charge".to_string(),
            TelemetryStatus::Error => "erreur".to_string(),
        };

        let fps = metrics.fps();
        let fps = if fps.is_finite() && fps > 0.0 {
            format!("{fps:.0}")
        } else {
            "collecte...".to_owned()
        };
        println!("Overlay: FPS {fps} | Jitter {jitter_str} | GPU {gpu_str}");
    }
}
