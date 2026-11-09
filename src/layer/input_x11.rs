use crate::layer::auto::{adjust_auto_target, adjust_hysteresis};
use crate::layer::config::{AutoMetric, Hotkey};
use crate::layer::diagnostics;
use crate::layer::limiter::FrameCapController;
use crate::layer::renderer::{
    OverlayFeedbackEvent, OverlayFeedbackState, OverlayModeState, OverlayRendererState,
};
use libloading::Library;
use parking_lot::Mutex;
use std::ffi::{c_char, c_int, c_long, c_uint, c_ulong};
use std::mem::MaybeUninit;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, Weak};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const KEY_PRESS: c_int = 2;
const KEY_RELEASE: c_int = 3;
const LONG_PRESS: Duration = Duration::from_secs(1);
const MODE_OFF: i32 = -1;
const MODE_MANUAL: i32 = 0;
const GRAB_MODE_ASYNC: c_int = 1;
const XK_F5: c_ulong = 0xffc2;
const XK_F6: c_ulong = 0xffc3;
const XK_F7: c_ulong = 0xffc4;
const XK_F8: c_ulong = 0xffc5;
const XK_CAPS_LOCK: c_ulong = 0xffe5;
const XK_NUM_LOCK: c_ulong = 0xff7f;
const XK_SCROLL_LOCK: c_ulong = 0xff14;

fn keysym(hotkey: Hotkey) -> c_ulong {
    match hotkey {
        Hotkey::F5 => XK_F5,
        Hotkey::F6 => XK_F6,
        Hotkey::F7 => XK_F7,
        Hotkey::F8 => XK_F8,
    }
}

#[repr(C)]
struct Display {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct XKeyEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut Display,
    window: c_ulong,
    root: c_ulong,
    subwindow: c_ulong,
    time: c_ulong,
    x: c_int,
    y: c_int,
    x_root: c_int,
    y_root: c_int,
    state: c_uint,
    keycode: c_uint,
    same_screen: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
union XEvent {
    type_: c_int,
    key: XKeyEvent,
    pad: [c_long; 24],
}

#[repr(C)]
struct XModifierKeymap {
    max_keypermod: c_int,
    modifiermap: *mut u8,
}

struct X11 {
    _library: Library,
    init_threads: unsafe extern "C" fn() -> c_int,
    open_display: unsafe extern "C" fn(*const c_char) -> *mut Display,
    close_display: unsafe extern "C" fn(*mut Display) -> c_int,
    default_root_window: unsafe extern "C" fn(*mut Display) -> c_ulong,
    keysym_to_keycode: unsafe extern "C" fn(*mut Display, c_ulong) -> u8,
    get_modifier_mapping: unsafe extern "C" fn(*mut Display) -> *mut XModifierKeymap,
    free_modifier_mapping: unsafe extern "C" fn(*mut XModifierKeymap) -> c_int,
    grab_key:
        unsafe extern "C" fn(*mut Display, c_int, c_uint, c_ulong, c_int, c_int, c_int) -> c_int,
    ungrab_key: unsafe extern "C" fn(*mut Display, c_int, c_uint, c_ulong) -> c_int,
    pending: unsafe extern "C" fn(*mut Display) -> c_int,
    next_event: unsafe extern "C" fn(*mut Display, *mut XEvent) -> c_int,
    peek_event: unsafe extern "C" fn(*mut Display, *mut XEvent) -> c_int,
    flush: unsafe extern "C" fn(*mut Display) -> c_int,
}

impl X11 {
    /// Loads the required Xlib entry points dynamically.
    fn load() -> Result<Self, String> {
        let library = unsafe { Library::new("libX11.so.6") }
            .map_err(|error| format!("unable to load libX11.so.6: {error}"))?;
        unsafe {
            Ok(Self {
                init_threads: *library
                    .get(b"XInitThreads\0")
                    .map_err(|error| format!("XInitThreads is unavailable: {error}"))?,
                open_display: *library
                    .get(b"XOpenDisplay\0")
                    .map_err(|error| format!("XOpenDisplay is unavailable: {error}"))?,
                close_display: *library
                    .get(b"XCloseDisplay\0")
                    .map_err(|error| format!("XCloseDisplay is unavailable: {error}"))?,
                default_root_window: *library
                    .get(b"XDefaultRootWindow\0")
                    .map_err(|error| format!("XDefaultRootWindow is unavailable: {error}"))?,
                keysym_to_keycode: *library
                    .get(b"XKeysymToKeycode\0")
                    .map_err(|error| format!("XKeysymToKeycode is unavailable: {error}"))?,
                get_modifier_mapping: *library
                    .get(b"XGetModifierMapping\0")
                    .map_err(|error| format!("XGetModifierMapping is unavailable: {error}"))?,
                free_modifier_mapping: *library
                    .get(b"XFreeModifiermap\0")
                    .map_err(|error| format!("XFreeModifiermap is unavailable: {error}"))?,
                grab_key: *library
                    .get(b"XGrabKey\0")
                    .map_err(|error| format!("XGrabKey is unavailable: {error}"))?,
                ungrab_key: *library
                    .get(b"XUngrabKey\0")
                    .map_err(|error| format!("XUngrabKey is unavailable: {error}"))?,
                pending: *library
                    .get(b"XPending\0")
                    .map_err(|error| format!("XPending is unavailable: {error}"))?,
                next_event: *library
                    .get(b"XNextEvent\0")
                    .map_err(|error| format!("XNextEvent is unavailable: {error}"))?,
                peek_event: *library
                    .get(b"XPeekEvent\0")
                    .map_err(|error| format!("XPeekEvent is unavailable: {error}"))?,
                flush: *library
                    .get(b"XFlush\0")
                    .map_err(|error| format!("XFlush is unavailable: {error}"))?,
                _library: library,
            })
        }
    }

    /// Resolves a supported function key to its X11 keycode.
    fn keycode(&self, display: *mut Display, hotkey: Hotkey) -> u8 {
        unsafe { (self.keysym_to_keycode)(display, keysym(hotkey)) }
    }

    /// Enumerates modifier combinations for Caps, Num, and Scroll Lock states.
    fn lock_modifier_masks(&self, display: *mut Display) -> Result<Vec<c_uint>, String> {
        let mapping = unsafe { (self.get_modifier_mapping)(display) };
        if mapping.is_null() {
            return Err("X11 modifier mapping is unavailable".to_owned());
        }
        let max_keys_per_modifier = unsafe { (*mapping).max_keypermod };
        if max_keys_per_modifier <= 0 {
            unsafe {
                (self.free_modifier_mapping)(mapping);
            }
            return Err("X11 modifier mapping is incomplete".to_owned());
        }
        let max_keypermod = max_keys_per_modifier as usize;
        let modifiermap_pointer = unsafe { (*mapping).modifiermap };
        let Some(mapping_len) = max_keypermod.checked_mul(8) else {
            unsafe {
                (self.free_modifier_mapping)(mapping);
            }
            return Err("X11 modifier mapping size overflow".to_owned());
        };
        if modifiermap_pointer.is_null() {
            unsafe {
                (self.free_modifier_mapping)(mapping);
            }
            return Err("X11 modifier mapping is incomplete".to_owned());
        }

        let lock_keycodes = [XK_CAPS_LOCK, XK_NUM_LOCK, XK_SCROLL_LOCK]
            .map(|keysym| unsafe { (self.keysym_to_keycode)(display, keysym) })
            .into_iter()
            .filter(|keycode| *keycode != 0)
            .collect::<Vec<_>>();
        let modifiermap = unsafe { std::slice::from_raw_parts(modifiermap_pointer, mapping_len) };
        let mut lock_mask: c_uint = 0;
        for modifier_index in 0..8 {
            let keycodes =
                &modifiermap[modifier_index * max_keypermod..(modifier_index + 1) * max_keypermod];
            if keycodes
                .iter()
                .any(|keycode| lock_keycodes.contains(keycode))
            {
                lock_mask |= 1u32 << modifier_index;
            }
        }
        unsafe {
            (self.free_modifier_mapping)(mapping);
        }

        let lock_bits = (0..8)
            .filter_map(|index| {
                let bit = 1u32 << index;
                (lock_mask & bit != 0).then_some(bit)
            })
            .collect::<Vec<_>>();
        let mut masks = vec![0];
        for bit in lock_bits {
            let combinations = masks.iter().map(|mask| mask | bit).collect::<Vec<_>>();
            masks.extend(combinations);
        }
        masks.sort_unstable();
        masks.dedup();
        Ok(masks)
    }
}

struct HotkeyTarget {
    controller: Arc<FrameCapController>,
    overlay_mode: Arc<OverlayModeState>,
    overlay_renderer: Arc<OverlayRendererState>,
    active_auto_algorithm: Arc<AtomicI32>,
    auto_hysteresis: Arc<Vec<AtomicU32>>,
    auto_targets: Arc<Vec<AtomicU64>>,
    auto_metrics: Vec<AutoMetric>,
    manual_step_fps: f32,
    overlay_feedback: Arc<OverlayFeedbackState>,
    hotkeys: [Hotkey; 4],
}

struct HotkeyListener {
    stop: Arc<AtomicBool>,
    worker: JoinHandle<()>,
}

#[derive(Default)]
struct HotkeyManager {
    targets: Vec<Weak<HotkeyTarget>>,
    listener: Option<HotkeyListener>,
}

fn hotkey_manager() -> &'static Mutex<HotkeyManager> {
    static MANAGER: OnceLock<Mutex<HotkeyManager>> = OnceLock::new();
    MANAGER.get_or_init(|| Mutex::new(HotkeyManager::default()))
}

pub struct HotkeyWorker {
    target: Option<Arc<HotkeyTarget>>,
}

impl HotkeyWorker {
    /// Registers a device with the process-wide X11 listener.
    pub fn start(
        enabled: bool,
        auto_key: Hotkey,
        up_key: Hotkey,
        down_key: Hotkey,
        toggle_key: Hotkey,
        controller: Arc<FrameCapController>,
        overlay_mode: Arc<OverlayModeState>,
        overlay_renderer: Arc<OverlayRendererState>,
        active_auto_algorithm: Arc<AtomicI32>,
        auto_hysteresis: Arc<Vec<AtomicU32>>,
        auto_targets: Arc<Vec<AtomicU64>>,
        auto_metrics: Vec<AutoMetric>,
        manual_step_fps: f32,
        overlay_feedback: Arc<OverlayFeedbackState>,
    ) -> Option<Self> {
        if !enabled {
            return None;
        }

        let target = Arc::new(HotkeyTarget {
            controller,
            overlay_mode,
            overlay_renderer,
            active_auto_algorithm,
            auto_hysteresis,
            auto_targets,
            auto_metrics,
            manual_step_fps,
            overlay_feedback,
            hotkeys: [auto_key, up_key, down_key, toggle_key],
        });
        let mut manager = hotkey_manager().lock();
        manager.targets.retain(|target| target.strong_count() > 0);
        manager.targets.push(Arc::downgrade(&target));

        if manager
            .listener
            .as_ref()
            .is_some_and(|listener| listener.worker.is_finished())
        {
            let listener = manager.listener.take().expect("listener was checked");
            let _ = listener.worker.join();
        }

        if manager.listener.is_none() {
            let stop = Arc::new(AtomicBool::new(false));
            let worker_stop = Arc::clone(&stop);
            let worker = thread::spawn(move || {
                if let Err(error) = run_listener(worker_stop) {
                    diagnostics::warn(format!(
                        "Framecap: F5/F6/F7/F8 X11 hotkeys unavailable: {error}"
                    ));
                }
            });
            manager.listener = Some(HotkeyListener { stop, worker });
        }

        Some(Self {
            target: Some(target),
        })
    }

    /// Unregisters a device and stops the listener when no devices remain.
    pub fn stop(&mut self) {
        let Some(target) = self.target.take() else {
            return;
        };
        let listener = {
            let mut manager = hotkey_manager().lock();
            manager.targets.retain(|registered| {
                registered
                    .upgrade()
                    .is_some_and(|registered| !Arc::ptr_eq(&registered, &target))
            });
            if manager.targets.is_empty() {
                manager.listener.take()
            } else {
                None
            }
        };

        if let Some(listener) = listener {
            listener.stop.store(true, Ordering::Relaxed);
            if listener.worker.join().is_err() {
                diagnostics::error(
                    "Framecap: F5/F6/F7/F8 hotkey listener thread terminated unexpectedly",
                );
            }
        }
    }
}

impl Drop for HotkeyWorker {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Grabs configured global keys once and dispatches them to every registered device.
fn run_listener(stop: Arc<AtomicBool>) -> Result<(), String> {
    let x11 = X11::load()?;
    if unsafe { (x11.init_threads)() } == 0 {
        return Err("XInitThreads failed".to_owned());
    }
    let display = unsafe { (x11.open_display)(ptr::null()) };
    if display.is_null() {
        return Err("cannot open DISPLAY (native Wayland is unsupported)".to_owned());
    }

    let root = unsafe { (x11.default_root_window)(display) };
    let hotkeys = [Hotkey::F5, Hotkey::F6, Hotkey::F7, Hotkey::F8];
    let keycodes = hotkeys.map(|hotkey| x11.keycode(display, hotkey));
    if root == 0 {
        unsafe {
            (x11.close_display)(display);
        }
        return Err("X11 root window is unavailable".to_owned());
    }
    let lock_masks = match x11.lock_modifier_masks(display) {
        Ok(_) if keycodes.contains(&0) => {
            unsafe {
                (x11.close_display)(display);
            }
            return Err("X11 did not resolve F5/F6/F7/F8 keycodes".to_owned());
        }
        Ok(masks) => masks,
        Err(error) => {
            unsafe {
                (x11.close_display)(display);
            }
            return Err(error);
        }
    };

    unsafe {
        for &modifier_mask in &lock_masks {
            for keycode in keycodes {
                (x11.grab_key)(
                    display,
                    i32::from(keycode),
                    modifier_mask,
                    root,
                    0,
                    GRAB_MODE_ASYNC,
                    GRAB_MODE_ASYNC,
                );
            }
        }
        (x11.flush)(display);
    }
    diagnostics::info("Framecap: process-wide X11 hotkey listener active (F5/F6/F7/F8)");

    let mut pressed: [Option<PressedKey>; 4] = [None; 4];
    while !stop.load(Ordering::Relaxed) {
        if unsafe { (x11.pending)(display) } == 0 {
            let now = Instant::now();
            for (index, key) in pressed.iter_mut().enumerate() {
                if key.as_mut().is_some_and(|key| key.long_press_due(now)) {
                    dispatch_hotkey(hotkeys[index], HotkeyEvent::LongPress);
                }
            }
            thread::sleep(Duration::from_millis(5));
            continue;
        }

        let mut event = MaybeUninit::<XEvent>::uninit();
        unsafe {
            (x11.next_event)(display, event.as_mut_ptr());
        }
        let event = unsafe { event.assume_init() };
        let key = unsafe { event.key };
        let Some(index) = keycodes
            .iter()
            .position(|keycode| u32::from(*keycode) == key.keycode)
        else {
            continue;
        };

        if key.type_ == KEY_PRESS && pressed[index].is_none() {
            pressed[index] = Some(PressedKey::new(key.time as u32, Instant::now()));
            dispatch_hotkey(hotkeys[index], HotkeyEvent::Press);
        } else if key.type_ == KEY_RELEASE {
            let mut next_event = MaybeUninit::<XEvent>::uninit();
            if unsafe { (x11.pending)(display) } > 0 {
                unsafe {
                    (x11.peek_event)(display, next_event.as_mut_ptr());
                }
                let next = unsafe { next_event.assume_init() };
                let next_key = unsafe { next.key };
                if next_key.type_ == KEY_PRESS
                    && next_key.keycode == key.keycode
                    && next_key.time == key.time
                {
                    unsafe {
                        (x11.next_event)(display, next_event.as_mut_ptr());
                    }
                    continue;
                }
            }
            if let Some(start) = pressed[index].take() {
                if let Some(event) = start.release(key.time as u32) {
                    dispatch_hotkey(hotkeys[index], event);
                }
            }
        }
    }

    unsafe {
        for &modifier_mask in &lock_masks {
            for keycode in keycodes {
                (x11.ungrab_key)(display, i32::from(keycode), modifier_mask, root);
            }
        }
        (x11.flush)(display);
        (x11.close_display)(display);
    }
    Ok(())
}

/// Applies the matching configured action to every active Vulkan device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HotkeyEvent {
    Press,
    ShortRelease,
    LongPress,
}

#[derive(Clone, Copy)]
struct PressedKey {
    x_time: u32,
    started_at: Instant,
    long_press_dispatched: bool,
}

impl PressedKey {
    fn new(x_time: u32, started_at: Instant) -> Self {
        Self {
            x_time,
            started_at,
            long_press_dispatched: false,
        }
    }

    fn long_press_due(&mut self, now: Instant) -> bool {
        if !self.long_press_dispatched && now.duration_since(self.started_at) >= LONG_PRESS {
            self.long_press_dispatched = true;
            true
        } else {
            false
        }
    }

    fn release(self, x_time: u32) -> Option<HotkeyEvent> {
        if self.long_press_dispatched {
            None
        } else if Duration::from_millis(u64::from(x_time.wrapping_sub(self.x_time))) >= LONG_PRESS {
            Some(HotkeyEvent::LongPress)
        } else {
            Some(HotkeyEvent::ShortRelease)
        }
    }
}

fn dispatch_hotkey(hotkey: Hotkey, event: HotkeyEvent) {
    let targets = {
        let mut manager = hotkey_manager().lock();
        let mut targets = Vec::with_capacity(manager.targets.len());
        manager.targets.retain(|target| {
            if let Some(target) = target.upgrade() {
                targets.push(target);
                true
            } else {
                false
            }
        });
        targets
    };

    for target in targets {
        dispatch_to_target(&target, hotkey, event);
    }
}

fn dispatch_to_target(target: &HotkeyTarget, hotkey: Hotkey, event: HotkeyEvent) {
    if let Some(action) = configured_action(&target.hotkeys, hotkey) {
        match (action, event) {
            (3, HotkeyEvent::LongPress) => {
                target
                    .overlay_feedback
                    .record(OverlayFeedbackEvent::HotkeyPressed(hotkey));
                let renderer = target.overlay_renderer.toggle();
                diagnostics::info(format!("[CONTROL] Overlay renderer {renderer:?}"));
            }
            (3, HotkeyEvent::ShortRelease) | (0..=2, HotkeyEvent::Press) => {
                dispatch_action(target, action, hotkey);
            }
            _ => {}
        }
    }
}

fn configured_action(hotkeys: &[Hotkey; 4], hotkey: Hotkey) -> Option<usize> {
    hotkeys.iter().position(|configured| *configured == hotkey)
}

fn dispatch_action(target: &HotkeyTarget, action: usize, hotkey: Hotkey) {
    target
        .overlay_feedback
        .record(OverlayFeedbackEvent::HotkeyPressed(hotkey));
    match action {
        0 => match cycle_regulation_mode(
            &target.active_auto_algorithm,
            &target.controller,
            target.auto_hysteresis.len(),
        ) {
            Ok(RegulationMode::Manual) => {
                target
                    .overlay_feedback
                    .record(OverlayFeedbackEvent::CapChanged);
                diagnostics::info("[CONTROL] FPS regulation mode MANUAL");
            }
            Ok(RegulationMode::Automatic(index)) => {
                diagnostics::info(format!(
                    "[CONTROL] FPS regulation mode AUTO ({})",
                    index + 1
                ));
            }
            Ok(RegulationMode::Off) => {
                target
                    .overlay_feedback
                    .record(OverlayFeedbackEvent::CapChanged);
                diagnostics::info("[CONTROL] FPS regulation mode OFF");
            }
            Err(error) => diagnostics::error(format!(
                "[CONTROL] unable to cycle FPS regulation mode: {error}"
            )),
        },
        1 => adjust_target(target, 1.0),
        2 => adjust_target(target, -1.0),
        3 => {
            let mode = target.overlay_mode.cycle();
            diagnostics::info(format!("[CONTROL] Overlay {mode:?}"));
        }
        _ => unreachable!("a configured hotkey has one of four actions"),
    }
}

fn adjust_target(target: &HotkeyTarget, direction: f32) {
    let active = target.active_auto_algorithm.load(Ordering::Relaxed);
    if active > 0 {
        let index = (active - 1) as usize;
        let Some(metric) = target.auto_metrics.get(index).copied() else {
            diagnostics::error("[CONTROL] active AUTO algorithm has no metric");
            return;
        };
        let change = match metric {
            AutoMetric::Fps => adjust_hysteresis(
                &target.auto_hysteresis,
                index,
                direction * metric.hysteresis_step(),
            )
            .map(|change| {
                (
                    change,
                    OverlayFeedbackEvent::HysteresisChanged,
                    "hysteresis",
                )
            }),
            AutoMetric::GpuVrmTempC | AutoMetric::FanRpm => {
                let Some(hysteresis) = target.auto_hysteresis.get(index) else {
                    diagnostics::error("[CONTROL] active AUTO algorithm has no hysteresis");
                    return;
                };
                let step = f32::from_bits(hysteresis.load(Ordering::Relaxed));
                adjust_auto_target(&target.auto_targets, index, direction * step)
                    .map(|change| (change, OverlayFeedbackEvent::TargetChanged, "target"))
            }
        };
        if let Some(((old, new), event, field)) = change {
            target.overlay_feedback.record(event);
            let verb = if direction > 0.0 { "raised" } else { "lowered" };
            diagnostics::info(format!(
                "[CONTROL] AUTO {field} {verb}: {old:.1} {} -> {new:.1} {}",
                metric.hysteresis_unit(),
                metric.hysteresis_unit()
            ));
        } else if metric != AutoMetric::Fps {
            diagnostics::warn("[CONTROL] AUTO target unchanged: step is zero or the new target is not a positive finite value");
        }
    } else if let Some((old, new)) = target.controller.adjust(direction * target.manual_step_fps) {
        target
            .overlay_feedback
            .record(OverlayFeedbackEvent::CapChanged);
        let verb = if direction > 0.0 { "raised" } else { "lowered" };
        diagnostics::info(format!(
            "[CONTROL] FPS Limit {verb}: {old:.1} -> {new:.1} fps"
        ));
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RegulationMode {
    Manual,
    Automatic(usize),
    Off,
}

/// Advances the manual/automatic/off cycle and restores the remembered target.
fn cycle_regulation_mode(
    active_auto_algorithm: &AtomicI32,
    controller: &FrameCapController,
    algorithm_count: usize,
) -> Result<RegulationMode, &'static str> {
    let active = active_auto_algorithm.load(Ordering::Relaxed);
    if active == MODE_OFF {
        controller
            .enable_last_target()
            .ok_or("the remembered FPS target is invalid")?;
        active_auto_algorithm.store(MODE_MANUAL, Ordering::Relaxed);
        return Ok(RegulationMode::Manual);
    }
    if active == MODE_MANUAL {
        if algorithm_count == 0 {
            return Err("no automatic algorithms are configured");
        }
        active_auto_algorithm.store(1, Ordering::Relaxed);
        return Ok(RegulationMode::Automatic(0));
    }
    let current_index = (active - 1) as usize;
    if current_index + 1 < algorithm_count {
        active_auto_algorithm.store(active + 1, Ordering::Relaxed);
        return Ok(RegulationMode::Automatic(current_index + 1));
    }
    controller.set_target_fps(None);
    active_auto_algorithm.store(-1, Ordering::Relaxed);
    Ok(RegulationMode::Off)
}

#[cfg(all(test, target_pointer_width = "64"))]
mod tests {
    use super::*;

    #[test]
    fn function_keys_resolve_to_f5_through_f8_x11_keysyms() {
        assert_eq!(
            [Hotkey::F5, Hotkey::F6, Hotkey::F7, Hotkey::F8].map(keysym),
            [0xffc2, 0xffc3, 0xffc4, 0xffc5]
        );
    }
    use std::mem::size_of;

    #[test]
    /// Checks that the Rust X event layouts match the Xlib ABI.
    fn x11_event_abi_layout_matches_xlib() {
        assert_eq!(size_of::<XKeyEvent>(), 96);
        assert_eq!(size_of::<XEvent>(), 192);
    }

    #[test]
    fn default_hotkeys_dispatch_expected_actions() {
        use crate::layer::renderer::OverlayMode;
        use crate::layer::settings::OverlayRenderer;

        let target = HotkeyTarget {
            controller: Arc::new(FrameCapController::new(Some(60.0))),
            overlay_mode: Arc::new(OverlayModeState::new(2)),
            overlay_renderer: Arc::new(OverlayRendererState::new(OverlayRenderer::Modern)),
            active_auto_algorithm: Arc::new(AtomicI32::new(0)),
            auto_hysteresis: Arc::new(vec![AtomicU32::new(5.0_f32.to_bits())]),
            auto_targets: Arc::new(vec![AtomicU64::new(0)]),
            auto_metrics: vec![AutoMetric::Fps],
            manual_step_fps: 5.0,
            overlay_feedback: Arc::new(OverlayFeedbackState::new()),
            hotkeys: [Hotkey::F5, Hotkey::F7, Hotkey::F6, Hotkey::F8],
        };
        dispatch_to_target(&target, Hotkey::F6, HotkeyEvent::Press);
        assert_eq!(target.controller.target_fps(), Some(55.0));
        dispatch_to_target(&target, Hotkey::F7, HotkeyEvent::Press);
        assert_eq!(target.controller.target_fps(), Some(60.0));
        dispatch_to_target(&target, Hotkey::F5, HotkeyEvent::Press);
        assert_eq!(target.active_auto_algorithm.load(Ordering::Relaxed), 1);
        dispatch_to_target(&target, Hotkey::F8, HotkeyEvent::ShortRelease);
        assert_eq!(target.overlay_mode.mode(), OverlayMode::Fps);
        dispatch_to_target(&target, Hotkey::F8, HotkeyEvent::LongPress);
        assert_eq!(target.overlay_renderer.current(), OverlayRenderer::Ascii);
    }

    #[test]
    fn f8_cycles_overlay_modes() {
        use crate::layer::renderer::OverlayMode;
        let mode = OverlayModeState::new(2);
        assert_eq!(mode.mode(), OverlayMode::Full);
        assert_eq!(mode.cycle(), OverlayMode::Fps);
        assert_eq!(mode.cycle(), OverlayMode::Off);
        assert_eq!(mode.cycle(), OverlayMode::Full);
        assert_eq!(OverlayModeState::new(0).cycle(), OverlayMode::Full);
        assert_eq!(OverlayModeState::new(1).cycle(), OverlayMode::Off);
    }

    #[test]
    /// Maps each physical function key to the configured action for that device.
    fn hotkey_dispatch_uses_each_devices_key_bindings() {
        let hotkeys = [Hotkey::F6, Hotkey::F8, Hotkey::F5, Hotkey::F7];

        assert_eq!(configured_action(&hotkeys, Hotkey::F6), Some(0));
        assert_eq!(configured_action(&hotkeys, Hotkey::F8), Some(1));
        assert_eq!(configured_action(&hotkeys, Hotkey::F5), Some(2));
        assert_eq!(configured_action(&hotkeys, Hotkey::F7), Some(3));
    }

    #[test]
    fn auto_hotkeys_adjust_sensor_targets_and_jit_hysteresis_independently() {
        use crate::layer::settings::OverlayRenderer;

        let target = HotkeyTarget {
            controller: Arc::new(FrameCapController::new(Some(60.0))),
            overlay_mode: Arc::new(OverlayModeState::new(2)),
            overlay_renderer: Arc::new(OverlayRendererState::new(OverlayRenderer::Modern)),
            active_auto_algorithm: Arc::new(AtomicI32::new(1)),
            auto_hysteresis: Arc::new(vec![
                AtomicU32::new(50.0_f32.to_bits()),
                AtomicU32::new(5.0_f32.to_bits()),
                AtomicU32::new(5.0_f32.to_bits()),
            ]),
            auto_targets: Arc::new(vec![
                AtomicU64::new(u64::from(2000.0_f32.to_bits())),
                AtomicU64::new(0),
                AtomicU64::new(u64::from(100.0_f32.to_bits())),
            ]),
            auto_metrics: vec![AutoMetric::FanRpm, AutoMetric::Fps, AutoMetric::GpuVrmTempC],
            manual_step_fps: 5.0,
            overlay_feedback: Arc::new(OverlayFeedbackState::new()),
            hotkeys: [Hotkey::F5, Hotkey::F7, Hotkey::F6, Hotkey::F8],
        };
        for (index, expected) in [(0, 2050.0), (1, 6.0), (2, 105.0)] {
            target
                .active_auto_algorithm
                .store(index as i32 + 1, Ordering::Relaxed);
            adjust_target(&target, 1.0);
            assert_eq!(
                if index == 1 {
                    f32::from_bits(target.auto_hysteresis[index].load(Ordering::Relaxed))
                } else {
                    f32::from_bits(target.auto_targets[index].load(Ordering::Relaxed) as u32)
                },
                expected
            );
            adjust_target(&target, -1.0);
        }
        assert_eq!(
            f32::from_bits(target.auto_hysteresis[0].load(Ordering::Relaxed)),
            50.0
        );
        assert_eq!(
            f32::from_bits(target.auto_hysteresis[1].load(Ordering::Relaxed)),
            5.0
        );
        assert_eq!(
            f32::from_bits(target.auto_hysteresis[2].load(Ordering::Relaxed)),
            5.0
        );
        assert_eq!(
            f32::from_bits(target.auto_targets[0].load(Ordering::Relaxed) as u32),
            2000.0
        );
        assert_eq!(
            f32::from_bits(target.auto_targets[2].load(Ordering::Relaxed) as u32),
            100.0
        );
        assert!(
            target
                .overlay_feedback
                .current(Instant::now())
                .hysteresis_changed
        );
        assert!(
            target
                .overlay_feedback
                .current(Instant::now())
                .target_changed
        );
        target.auto_hysteresis[0].store(0.0_f32.to_bits(), Ordering::Relaxed);
        target.active_auto_algorithm.store(1, Ordering::Relaxed);
        adjust_target(&target, -1.0);
        assert_eq!(
            f32::from_bits(target.auto_targets[0].load(Ordering::Relaxed) as u32),
            2000.0
        );
        target.active_auto_algorithm.store(4, Ordering::Relaxed);
        adjust_target(&target, 1.0);
        assert_eq!(target.controller.target_fps(), Some(60.0));
    }

    #[test]
    fn sensor_hotkeys_update_the_regulators_shared_target() {
        use crate::layer::auto::AutoRegulator;
        use crate::layer::config::{AutoAlgorithmConfig, AutoTarget};
        use crate::layer::settings::OverlayRenderer;

        let config = AutoAlgorithmConfig {
            name: "vrm_temp".to_owned(),
            metric: AutoMetric::GpuVrmTempC,
            target: AutoTarget::Value(100.0),
            hysteresis: 7.5,
            window: Duration::from_secs(4),
            increase_step_fps: 5.0,
            decrease_step_fps: 5.0,
            min_fps: 1.0,
            max_fps: 240.0,
        };
        let regulator = AutoRegulator::new(vec![config], 98.0, 90.0, 2.0);
        let target = HotkeyTarget {
            controller: Arc::new(FrameCapController::new(Some(60.0))),
            overlay_mode: Arc::new(OverlayModeState::new(2)),
            overlay_renderer: Arc::new(OverlayRendererState::new(OverlayRenderer::Modern)),
            active_auto_algorithm: Arc::new(AtomicI32::new(1)),
            auto_hysteresis: Arc::new(vec![AtomicU32::new(7.5_f32.to_bits())]),
            auto_targets: regulator.targets(),
            auto_metrics: vec![AutoMetric::GpuVrmTempC],
            manual_step_fps: 5.0,
            overlay_feedback: Arc::new(OverlayFeedbackState::new()),
            hotkeys: [Hotkey::F5, Hotkey::F7, Hotkey::F6, Hotkey::F8],
        };
        dispatch_to_target(&target, Hotkey::F7, HotkeyEvent::Press);
        assert_eq!(
            regulator.overlay_target(0),
            Some((AutoMetric::GpuVrmTempC, AutoTarget::Value(107.5)))
        );
        assert!(
            target
                .overlay_feedback
                .current(Instant::now())
                .target_changed
        );
        assert_eq!(
            f32::from_bits(target.auto_hysteresis[0].load(Ordering::Relaxed)),
            7.5
        );
        dispatch_to_target(&target, Hotkey::F6, HotkeyEvent::Press);
        assert_eq!(
            regulator.overlay_target(0),
            Some((AutoMetric::GpuVrmTempC, AutoTarget::Value(100.0)))
        );
    }

    #[test]
    fn overlay_hotkey_short_and_long_presses_are_independent() {
        use crate::layer::renderer::OverlayMode;
        use crate::layer::settings::OverlayRenderer;

        let target = HotkeyTarget {
            controller: Arc::new(FrameCapController::new(Some(60.0))),
            overlay_mode: Arc::new(OverlayModeState::new(2)),
            overlay_renderer: Arc::new(OverlayRendererState::new(OverlayRenderer::Modern)),
            active_auto_algorithm: Arc::new(AtomicI32::new(0)),
            auto_hysteresis: Arc::new(Vec::new()),
            auto_targets: Arc::new(Vec::new()),
            auto_metrics: Vec::new(),
            manual_step_fps: 5.0,
            overlay_feedback: Arc::new(OverlayFeedbackState::new()),
            hotkeys: [Hotkey::F6, Hotkey::F8, Hotkey::F5, Hotkey::F7],
        };
        let dispatch = |event| dispatch_to_target(&target, Hotkey::F7, event);
        let now = Instant::now();
        let mut key = PressedKey::new(100, now);
        dispatch(HotkeyEvent::Press);
        assert_eq!(target.overlay_mode.mode(), OverlayMode::Full);
        assert!(!key.long_press_due(now + Duration::from_millis(999)));
        dispatch(key.release(1099).unwrap());
        assert_eq!(target.overlay_mode.mode(), OverlayMode::Fps);
        assert_eq!(target.overlay_renderer.current(), OverlayRenderer::Modern);

        let mut key = PressedKey::new(2000, now);
        dispatch(HotkeyEvent::Press);
        assert!(key.long_press_due(now + LONG_PRESS));
        dispatch(HotkeyEvent::LongPress);
        assert_eq!(target.overlay_mode.mode(), OverlayMode::Fps);
        assert_eq!(target.overlay_renderer.current(), OverlayRenderer::Ascii);
        assert!(!key.long_press_due(now + Duration::from_secs(3)));
        assert_eq!(key.release(5000), None);
        assert_eq!(
            target
                .overlay_feedback
                .current(std::time::Instant::now())
                .hotkey,
            Some(Hotkey::F7)
        );

        dispatch(PressedKey::new(6000, now).release(6010).unwrap());
        assert_eq!(target.overlay_mode.mode(), OverlayMode::Off);
        let mut key = PressedKey::new(7000, now);
        assert!(key.long_press_due(now + LONG_PRESS));
        dispatch(HotkeyEvent::LongPress);
        assert_eq!(target.overlay_mode.mode(), OverlayMode::Off);
        assert_eq!(target.overlay_renderer.current(), OverlayRenderer::Modern);
        assert_eq!(key.release(8000), None);

        dispatch_to_target(&target, Hotkey::F8, HotkeyEvent::Press);
        dispatch_to_target(&target, Hotkey::F8, HotkeyEvent::LongPress);
        dispatch_to_target(&target, Hotkey::F8, HotkeyEvent::ShortRelease);
        assert_eq!(target.controller.target_fps(), Some(65.0));
        assert_eq!(target.overlay_renderer.current(), OverlayRenderer::Modern);
    }

    #[test]
    fn queued_releases_use_x11_timestamps_and_handle_wraparound() {
        let now = Instant::now();
        assert_eq!(
            PressedKey::new(u32::MAX - 499, now).release(499),
            Some(HotkeyEvent::ShortRelease)
        );
        assert_eq!(
            PressedKey::new(u32::MAX - 499, now).release(500),
            Some(HotkeyEvent::LongPress)
        );
    }

    #[test]
    /// Checks cycling through every AUTO algorithm before OFF and manual restoration.
    fn f5_cycles_all_auto_algorithms_and_restores_the_target() {
        let mode = AtomicI32::new(0);
        let controller = FrameCapController::new(Some(75.0));

        assert_eq!(
            cycle_regulation_mode(&mode, &controller, 3),
            Ok(RegulationMode::Automatic(0))
        );
        assert_eq!(mode.load(Ordering::Relaxed), 1);
        assert_eq!(controller.target_fps(), Some(75.0));

        assert_eq!(
            cycle_regulation_mode(&mode, &controller, 3),
            Ok(RegulationMode::Automatic(1))
        );
        assert_eq!(mode.load(Ordering::Relaxed), 2);

        assert_eq!(
            cycle_regulation_mode(&mode, &controller, 3),
            Ok(RegulationMode::Automatic(2))
        );
        assert_eq!(mode.load(Ordering::Relaxed), 3);

        assert_eq!(
            cycle_regulation_mode(&mode, &controller, 3),
            Ok(RegulationMode::Off)
        );
        assert_eq!(mode.load(Ordering::Relaxed), -1);
        assert_eq!(controller.target_fps(), None);

        assert_eq!(
            cycle_regulation_mode(&mode, &controller, 3),
            Ok(RegulationMode::Manual)
        );
        assert_eq!(mode.load(Ordering::Relaxed), 0);
        assert_eq!(controller.target_fps(), Some(75.0));

        let initially_off = FrameCapController::new(None);
        let initially_off_mode = AtomicI32::new(-1);
        assert_eq!(
            cycle_regulation_mode(&initially_off_mode, &initially_off, 3),
            Ok(RegulationMode::Manual)
        );
        assert_eq!(initially_off.target_fps(), Some(60.0));
    }
}
