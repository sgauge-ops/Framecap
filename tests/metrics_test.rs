use framecap::layer::metrics::MetricsAggregator;
use framecap::layer::overlay::Overlay;
use framecap::layer::telemetry::MockGpuProvider;
use std::time::Duration;

#[test]
/// Checks jitter values while the fixed sample window fills.
fn test_jitter_calculation() {
    let provider = Box::new(MockGpuProvider { mock_value: 0.0 });
    let mut aggregator = MetricsAggregator::new(60, provider);

    // Add samples with constant interval (16.67ms)
    for _ in 0..60 {
        aggregator.add_sample(Duration::from_millis(16));
    }

    let snapshot = aggregator.add_sample(Duration::from_millis(16));
    Overlay::new().render(&snapshot);

    // With constant intervals, jitter should be very low (near 0)
    assert!(snapshot.jitter_ms() < 1.0);
    assert_eq!(snapshot.frametime_ms(), 16.0);
    assert_eq!(snapshot.gpu_measurement().device_id, Some(0));
    assert!(snapshot.gpu_measurement().timestamp <= snapshot.timestamp());
}

#[test]
/// Checks the transition from collection to available jitter metrics.
fn test_jitter_collection_state() {
    let provider = Box::new(MockGpuProvider { mock_value: 0.0 });
    let mut aggregator = MetricsAggregator::new(60, provider);

    // Add only 30 samples (less than window_size 60)
    for _ in 0..30 {
        aggregator.add_sample(Duration::from_millis(16));
    }

    let snapshot = aggregator.add_sample(Duration::from_millis(16));

    // Jitter should be marked as "collecting" (-1.0)
    assert_eq!(snapshot.jitter_ms(), -1.0);
}

#[test]
/// Checks that resetting the aggregator clears collected samples.
fn test_metrics_reset() {
    let provider = Box::new(MockGpuProvider { mock_value: 0.0 });
    let mut aggregator = MetricsAggregator::new(60, provider);

    for _ in 0..60 {
        aggregator.add_sample(Duration::from_millis(16));
    }

    aggregator.reset();

    let snapshot = aggregator.add_sample(Duration::from_millis(16));
    // After reset, it should be in collection state again
    assert_eq!(snapshot.jitter_ms(), -1.0);
}

#[test]
/// Checks that low-10 FPS appears after the configured window fills.
fn test_low_10_fps_is_reported_after_window_is_filled() {
    let provider = Box::new(MockGpuProvider { mock_value: 35.0 });
    let mut aggregator = MetricsAggregator::new(60, provider);

    for _ in 0..60 {
        aggregator.add_sample(Duration::from_millis(16));
    }

    let snapshot = aggregator.add_sample(Duration::from_millis(16));
    assert!(snapshot.low_10_fps().is_finite());
    assert!(snapshot.low_10_fps() > 0.0);
}
