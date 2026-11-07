use crate::layer::config::{DEFAULT_TARGET_FPS, MAX_TARGET_FPS, MIN_TARGET_FPS};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub struct FrameCapController {
    target_fps_bits: AtomicU32,
    target_enabled: AtomicBool,
    generation: AtomicU64,
    manual_min_fps: f32,
    manual_max_fps: f32,
}

impl FrameCapController {
    /// Creates a controller with the built-in manual FPS bounds.
    pub fn new(target_fps: Option<f32>) -> Self {
        Self::with_manual_limits(target_fps, MIN_TARGET_FPS, MAX_TARGET_FPS)
    }

    /// Creates a controller with explicit manual FPS bounds.
    pub fn with_manual_limits(
        target_fps: Option<f32>,
        manual_min_fps: f32,
        manual_max_fps: f32,
    ) -> Self {
        Self {
            target_fps_bits: AtomicU32::new(target_fps.unwrap_or(DEFAULT_TARGET_FPS).to_bits()),
            target_enabled: AtomicBool::new(target_fps.is_some()),
            generation: AtomicU64::new(0),
            manual_min_fps,
            manual_max_fps,
        }
    }

    /// Returns the generation counter used to invalidate presentation history.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    /// Returns the active target, or `None` when limiting is disabled.
    pub fn target_fps(&self) -> Option<f32> {
        if !self.target_enabled.load(Ordering::Relaxed) {
            return None;
        }
        let fps = f32::from_bits(self.target_fps_bits.load(Ordering::Relaxed));
        (fps > 0.0).then_some(fps)
    }

    /// Enables, changes, or disables the target and reports whether it changed.
    pub fn set_target_fps(&self, target_fps: Option<f32>) -> bool {
        if target_fps.is_some_and(|fps| {
            !fps.is_finite() || !(MIN_TARGET_FPS..=MAX_TARGET_FPS).contains(&fps)
        }) {
            return false;
        }
        let changed = match target_fps {
            Some(fps) => {
                let bits = fps.to_bits();
                let target_changed = self.target_fps_bits.swap(bits, Ordering::Relaxed) != bits;
                let enabled_changed = !self.target_enabled.swap(true, Ordering::Relaxed);
                target_changed || enabled_changed
            }
            None => self.target_enabled.swap(false, Ordering::Relaxed),
        };
        if changed {
            self.generation.fetch_add(1, Ordering::Relaxed);
        }
        changed
    }

    /// Re-enables the previously stored target when it remains valid.
    pub fn enable_last_target(&self) -> Option<f32> {
        let fps = f32::from_bits(self.target_fps_bits.load(Ordering::Relaxed));
        if !fps.is_finite() || !(MIN_TARGET_FPS..=MAX_TARGET_FPS).contains(&fps) {
            return None;
        }
        if !self.target_enabled.swap(true, Ordering::Relaxed) {
            self.generation.fetch_add(1, Ordering::Relaxed);
        }
        Some(fps)
    }

    /// Adjusts the active target using the controller's configured manual bounds.
    pub fn adjust(&self, delta: f32) -> Option<(f32, f32)> {
        self.adjust_with_bounds(delta, self.manual_min_fps, self.manual_max_fps)
    }

    /// Atomically changes the active target within the supplied FPS bounds.
    pub fn adjust_with_bounds(&self, delta: f32, min_fps: f32, max_fps: f32) -> Option<(f32, f32)> {
        if !delta.is_finite() || delta == 0.0 || !self.target_enabled.load(Ordering::Relaxed) {
            return None;
        }
        if !min_fps.is_finite()
            || !max_fps.is_finite()
            || !(MIN_TARGET_FPS..=MAX_TARGET_FPS).contains(&min_fps)
            || !(MIN_TARGET_FPS..=MAX_TARGET_FPS).contains(&max_fps)
            || min_fps > max_fps
        {
            return None;
        }
        let mut current_bits = self.target_fps_bits.load(Ordering::Relaxed);
        loop {
            let current = f32::from_bits(current_bits);
            if current <= 0.0 || !current.is_finite() {
                return None;
            }
            if (delta > 0.0 && current >= max_fps) || (delta < 0.0 && current <= min_fps) {
                return None;
            }
            let next = if delta > 0.0 {
                (current + delta).min(max_fps).max(min_fps)
            } else {
                (current + delta).max(min_fps).min(max_fps)
            };
            if next == current {
                return None;
            }
            match self.target_fps_bits.compare_exchange_weak(
                current_bits,
                next.to_bits(),
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    self.generation.fetch_add(1, Ordering::Relaxed);
                    return Some((current, next));
                }
                Err(actual) => current_bits = actual,
            }
        }
    }
}

#[derive(Default)]
pub struct FrameScheduler {
    target_fps: Option<f32>,
    next_deadline: Option<Instant>,
}

impl FrameScheduler {
    /// Returns the next pacing deadline for a target, skipping missed periods.
    pub fn next_deadline(&mut self, now: Instant, target_fps: Option<f32>) -> Option<Instant> {
        let Some(target_fps) = target_fps else {
            self.target_fps = None;
            self.next_deadline = None;
            return None;
        };
        if !target_fps.is_finite() || !(MIN_TARGET_FPS..=MAX_TARGET_FPS).contains(&target_fps) {
            self.target_fps = None;
            self.next_deadline = None;
            return None;
        }

        if self.target_fps != Some(target_fps) || self.next_deadline.is_none() {
            self.target_fps = Some(target_fps);
            self.next_deadline = now.checked_add(frame_period(target_fps));
            return None;
        }

        let period = frame_period(target_fps);
        let deadline = self.next_deadline?;
        if deadline <= now {
            if now.duration_since(deadline) >= period {
                self.next_deadline = now.checked_add(period);
            } else {
                self.next_deadline = deadline.checked_add(period);
            }
            return None;
        }

        self.next_deadline = deadline.checked_add(period);
        Some(deadline)
    }
}

/// Converts a positive FPS target to its frame period.
fn frame_period(target_fps: f32) -> Duration {
    Duration::from_secs_f64(1.0 / f64::from(target_fps))
}

/// Sleeps until a monotonic deadline, retrying interrupted sleeps.
pub fn wait_until(deadline: Instant) -> std::io::Result<()> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Ok(());
    }

    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let target = add_duration(now, remaining)?;

    loop {
        let result = unsafe {
            libc::clock_nanosleep(
                libc::CLOCK_MONOTONIC,
                libc::TIMER_ABSTIME,
                &target,
                std::ptr::null_mut(),
            )
        };
        if result == 0 {
            return Ok(());
        }
        if result != libc::EINTR {
            return Err(std::io::Error::from_raw_os_error(result));
        }
    }
}

/// Adds a duration to a monotonic timespec with overflow checks.
fn add_duration(time: libc::timespec, duration: Duration) -> std::io::Result<libc::timespec> {
    let seconds = i64::try_from(duration.as_secs()).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "deadline duration overflow",
        )
    })?;
    let nanos = time.tv_nsec + i64::from(duration.subsec_nanos());
    let extra_second = nanos / 1_000_000_000;
    let tv_sec = time
        .tv_sec
        .checked_add(seconds)
        .and_then(|value| value.checked_add(extra_second))
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "deadline overflow")
        })?;
    Ok(libc::timespec {
        tv_sec,
        tv_nsec: (nanos % 1_000_000_000) as libc::c_long,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Checks five-FPS adjustment and clamping to manual limits.
    fn controller_adjusts_in_five_fps_steps_and_clamps_to_bounds() {
        let controller = FrameCapController::new(Some(60.0));
        assert_eq!(controller.adjust(-5.0), Some((60.0, 55.0)));
        assert_eq!(controller.adjust(5.0), Some((55.0, 60.0)));

        let low = FrameCapController::new(Some(1.0));
        assert_eq!(low.adjust(-5.0), None);
        assert_eq!(low.target_fps(), Some(1.0));

        let high = FrameCapController::new(Some(240.0));
        assert_eq!(high.adjust(5.0), None);
        assert_eq!(high.target_fps(), Some(240.0));
    }

    #[test]
    /// Checks that configured manual step size and bounds are honored.
    fn controller_uses_configured_manual_step_bounds() {
        let controller = FrameCapController::with_manual_limits(Some(19.0), 20.0, 100.0);
        assert_eq!(controller.adjust(2.5), Some((19.0, 21.5)));
        assert_eq!(controller.adjust(-2.5), Some((21.5, 20.0)));
        assert_eq!(controller.adjust(-2.5), None);

        let maximum = FrameCapController::with_manual_limits(Some(99.0), 20.0, 100.0);
        assert_eq!(maximum.adjust(5.0), Some((99.0, 100.0)));
        assert_eq!(maximum.adjust(5.0), None);
    }

    #[test]
    /// Checks that manual adjustment does not re-enable a disabled cap.
    fn controller_adjustment_does_not_enable_a_disabled_cap() {
        let controller = FrameCapController::new(None);
        assert_eq!(controller.adjust(5.0), None);
        assert_eq!(controller.adjust(-5.0), None);
        assert_eq!(controller.target_fps(), None);
    }

    #[test]
    /// Checks restoration of the last target after disabling the cap.
    fn controller_restores_the_last_target_after_disabling_the_cap() {
        let controller = FrameCapController::new(Some(75.0));
        assert!(controller.set_target_fps(None));
        assert_eq!(controller.target_fps(), None);
        assert_eq!(controller.adjust(5.0), None);
        assert_eq!(controller.enable_last_target(), Some(75.0));
        assert_eq!(controller.target_fps(), Some(75.0));
        assert_eq!(controller.generation(), 2);
    }

    #[test]
    /// Checks rejection of non-finite and out-of-range target values.
    fn controller_rejects_invalid_targets() {
        let controller = FrameCapController::new(Some(60.0));
        assert!(!controller.set_target_fps(Some(f32::NAN)));
        assert!(!controller.set_target_fps(Some(0.5)));
        assert!(!controller.set_target_fps(Some(241.0)));
        assert_eq!(controller.target_fps(), Some(60.0));
        assert_eq!(controller.generation(), 0);
    }

    #[test]
    /// Checks that the generation counter changes only with the target state.
    fn controller_generation_advances_only_when_the_target_changes() {
        let controller = FrameCapController::new(Some(60.0));
        assert_eq!(controller.generation(), 0);
        assert_eq!(controller.adjust(5.0), Some((60.0, 65.0)));
        assert_eq!(controller.generation(), 1);
        assert_eq!(controller.adjust(5.0), Some((65.0, 70.0)));
        assert_eq!(controller.generation(), 2);
        assert!(!controller.set_target_fps(Some(70.0)));
        assert_eq!(controller.generation(), 2);
        assert!(controller.set_target_fps(None));
        assert_eq!(controller.generation(), 3);
    }

    #[test]
    /// Checks the initial wait and absolute cadence of the scheduler.
    fn scheduler_waits_for_the_first_period_then_keeps_absolute_cadence() {
        let start = Instant::now();
        let period = frame_period(60.0);
        let mut scheduler = FrameScheduler::default();

        assert_eq!(scheduler.next_deadline(start, Some(60.0)), None);
        assert_eq!(
            scheduler.next_deadline(start + Duration::from_millis(5), Some(60.0)),
            Some(start + period)
        );
        assert_eq!(
            scheduler.next_deadline(start + period, Some(60.0)),
            Some(start + period * 2)
        );
    }

    #[test]
    /// Checks that missed frame deadlines are skipped without catch-up bursts.
    fn scheduler_skips_missed_deadlines_without_catching_up() {
        let start = Instant::now();
        let period = frame_period(60.0);
        let mut scheduler = FrameScheduler::default();

        scheduler.next_deadline(start, Some(60.0));
        assert_eq!(
            scheduler.next_deadline(start + period * 3, Some(60.0)),
            None
        );
        assert_eq!(
            scheduler.next_deadline(start + period * 3, Some(60.0)),
            Some(start + period * 4)
        );
    }

    #[test]
    /// Checks scheduler reset when the target changes or limiting is disabled.
    fn scheduler_resets_when_target_changes_or_is_disabled() {
        let start = Instant::now();
        let mut scheduler = FrameScheduler::default();

        scheduler.next_deadline(start, Some(60.0));
        assert_eq!(
            scheduler.next_deadline(start + Duration::from_millis(5), Some(30.0)),
            None
        );
        assert_eq!(
            scheduler.next_deadline(start + Duration::from_millis(10), None),
            None
        );
        assert_eq!(
            scheduler.next_deadline(start + Duration::from_millis(11), Some(30.0)),
            None
        );
    }
}
