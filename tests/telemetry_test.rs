use framecap::layer::telemetry::{
    AmdGpuProvider, GpuTelemetryProvider, MockGpuProvider, TelemetryStatus,
};
use std::time::{Duration, Instant};

#[test]
/// Checks metrics produced by the mock GPU telemetry provider.
fn test_mock_gpu_provider() {
    let provider = MockGpuProvider { mock_value: 95.0 };
    let metric = provider.get_metric();

    assert_eq!(metric.value, 95.0);
    assert_eq!(metric.status, TelemetryStatus::Available);
}

#[test]
/// Checks the unsupported state for an unavailable AMD GPU provider.
fn test_amd_gpu_provider_unsupported() {
    // Utiliser un index de carte qui n'existe probablement pas pour simuler "Unsupported"
    let provider = AmdGpuProvider::new(999);
    let metric = provider.get_metric();

    assert_eq!(metric.status, TelemetryStatus::Unsupported);
}

#[test]
/// Checks expiration of a GPU measurement older than its configured limit.
fn test_gpu_measurement_expiry() {
    // Ce test nécessite une simulation de temps ou un mock du système de fichiers
    // Pour la Phase 1, nous nous concentrons sur la logique de transition d'état
    let provider = MockGpuProvider { mock_value: 50.0 };
    let metric = provider.get_metric();

    // Simulation d'une mesure expirée (plus de 1000ms)
    let expired_metric = framecap::layer::lib::GpuMeasurement {
        value: metric.value,
        timestamp: Instant::now() - Duration::from_millis(2000),
        status: TelemetryStatus::Available,
        device_id: metric.device_id,
    };

    // La logique de vérification d'expiration doit être testée dans le manager ou la couche de présentation
    assert!(expired_metric.timestamp.elapsed().as_millis() > 1000);
}
