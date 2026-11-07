use crate::layer::config::Config;
use crate::layer::lib::FramecapLayer;
use crate::layer::metrics::MetricsAggregator;
use crate::layer::telemetry::{AmdGpuProvider, GpuTelemetryProvider};
use std::sync::Arc;

/// Représente l'état interne de la Layer pour une queue de présentation.
pub struct QueueState {
    pub metrics_aggregator: MetricsAggregator,
    pub last_present_time: std::time::Instant,
}

impl QueueState {
    /// Creates queue-local metrics state and initializes its presentation clock.
    pub fn new(window_size: usize, provider: Box<dyn GpuTelemetryProvider>) -> Self {
        Self {
            metrics_aggregator: MetricsAggregator::new(window_size, provider),
            last_present_time: std::time::Instant::now(),
        }
    }

    /// Resets the timestamp used to measure the next presentation interval.
    pub fn update_last_present(&mut self) {
        self.last_present_time = std::time::Instant::now();
    }
}

/// Points d'entrée pour la Vulkan Layer.
/// En Phase 1, ces fonctions initialisent la couche et collectent les métriques.
/// Builds the shared layer state from the active configuration.
pub fn init_layer() -> Arc<FramecapLayer> {
    let config = Config::load();
    let provider = Box::new(AmdGpuProvider::new(0));

    // Note: En production, l'initialisation de la Layer doit être thread-safe.
    // Ici, on prépare la structure de base.
    let layer = FramecapLayer {
        metrics_aggregator: MetricsAggregator::new(config.window_size, provider),
        telemetry_provider: Arc::new(AmdGpuProvider::new(0)), // Ceci est une simplification pour l'exemple
        overlay: crate::layer::overlay::Overlay::new(),
        renderer: crate::layer::renderer::Renderer::new(),
        config,
        poll_count: 0,
    };

    Arc::new(layer)
}

/// Intercepte l'appel vkQueuePresentKHR.
/// Calcule l'intervalle de présentation, met à jour les métriques et appelle la Layer aval.
/// Updates the metrics for a presentation using the queue's previous timestamp.
pub fn handle_queue_present(layer: &mut FramecapLayer, last_present_time: &mut std::time::Instant) {
    let now = std::time::Instant::now();
    let interval = now.duration_since(*last_present_time);

    // Mise à jour des métriques
    layer.handle_present(interval);

    // Mise à jour du temps de la dernière présentation
    *last_present_time = now;

    // Ici, on appellerait normalement la fonction de la Layer aval.
    // println!("Passage à la Layer aval...");
}
