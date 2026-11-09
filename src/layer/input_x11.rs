use crate::layer::auto::adjust_hysteresis_percent;
use crate::layer::config::Hotkey;
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
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::sync::{Arc, OnceLock, Weak};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const KEY_PRESS: c_int = 2;
const KEY_RELEASE: c_int = 3;
const LONG_PRESS_MS: u32 = 1000;
const MODE_OFF: i32 = -1;
const MODE_MANUAL: i32 = 0;
const GRAB_MODE_ASYNC: c_int = 1;
const XK_F9: c_ulong = 0xffc6;
const XK_F10: c_ulong = 0xffc7;
const XK_F11: c_ulong = 0xffc8;
const XK_F12: c_ulong = 0xffc9;
const XK_CAPS_LOCK: c_ulong = 0xffe5;
const XK_NUM_LOCK: c_ulong = 0xff7f;
const XK_SCROLL_LOCK: c_ulong = 0xff14;

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
        let keysym = match hotkey {
            Hotkey::F9 => XK_F9,
            Hotkey::F10 => XK_F10,
            Hotkey::F11 => XK_F11,
            Hotkey::F12 => XK_F12,
        };
        unsafe { (self.keysym_to_keycode)(display, keysym) }
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
    auto_hysteresis_percent: Arc<Vec<AtomicU32>>,
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
        auto_hysteresis_percent: Arc<Vec<AtomicU32>>,
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
            auto_hysteresis_percent,
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
                        "Framecap: F9/F10/F11/F12 X11 hotkeys unavailable: {error}"
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
                    "Framecap: F9/F10/F11/F12 hotkey listener thread terminated unexpectedly",
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
    let hotkeys = [Hotkey::F9, Hotkey::F10, Hotkey::F11, Hotkey::F12];
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
            return Err("X11 did not resolve F9/F10/F11/F12 keycodes".to_owned());
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
    diagnostics::info("Framecap: process-wide X11 hotkey listener active (F9/F10/F11/F12)");

    let mut pressed = [None; 4];
    while !stop.load(Ordering::Relaxed) {
        if unsafe { (x11.pending)(display) } == 0 {
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
            pressed[index] = Some(key.time as u32);
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
                let elapsed = (key.time as u32).wrapping_sub(start);
                dispatch_hotkey(
                    hotkeys[index],
                    HotkeyEvent::Release {
                        elapsed_ms: elapsed,
                    },
                );
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
    Release { elapsed_ms: u32 },
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
            (3, HotkeyEvent::Release { elapsed_ms }) if elapsed_ms >= LONG_PRESS_MS => {
                target
                    .overlay_feedback
                    .record(OverlayFeedbackEvent::HotkeyPressed(hotkey));
                let renderer = target.overlay_renderer.toggle();
                diagnostics::info(format!("[CONTROL] Overlay renderer {renderer:?}"));
            }
            (3, HotkeyEvent::Release { .. }) | (0..=2, HotkeyEvent::Press) => {
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
            target.auto_hysteresis_percent.len(),
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
        if let Some((old, new)) = adjust_hysteresis_percent(
            &target.auto_hysteresis_percent,
            (active - 1) as usize,
            direction,
        ) {
            target
                .overlay_feedback
                .record(OverlayFeedbackEvent::HysteresisChanged);
            let verb = if direction > 0.0 { "raised" } else { "lowered" };
            diagnostics::info(format!(
                "[CONTROL] AUTO hysteresis {verb}: {old:.1}% -> {new:.1}%"
            ));
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
    use std::mem::size_of;

    #[test]
    /// Checks that the Rust X event layouts match the Xlib ABI.
    fn x11_event_abi_layout_matches_xlib() {
        assert_eq!(size_of::<XKeyEvent>(), 96);
        assert_eq!(size_of::<XEvent>(), 192);
    }

    #[test]
    fn f12_cycles_overlay_modes() {
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
        let hotkeys = [Hotkey::F10, Hotkey::F12, Hotkey::F9, Hotkey::F11];

        assert_eq!(configured_action(&hotkeys, Hotkey::F10), Some(0));
        assert_eq!(configured_action(&hotkeys, Hotkey::F12), Some(1));
        assert_eq!(configured_action(&hotkeys, Hotkey::F9), Some(2));
        assert_eq!(configured_action(&hotkeys, Hotkey::F11), Some(3));
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
            auto_hysteresis_percent: Arc::new(Vec::new()),
            manual_step_fps: 5.0,
            overlay_feedback: Arc::new(OverlayFeedbackState::new()),
            hotkeys: [Hotkey::F10, Hotkey::F12, Hotkey::F9, Hotkey::F11],
        };
        let press = |event| dispatch_to_target(&target, Hotkey::F11, event);
        press(HotkeyEvent::Press);
        assert_eq!(target.overlay_mode.mode(), OverlayMode::Full);
        press(HotkeyEvent::Release {
            elapsed_ms: LONG_PRESS_MS - 1,
        });
        assert_eq!(target.overlay_mode.mode(), OverlayMode::Fps);
        assert_eq!(target.overlay_renderer.current(), OverlayRenderer::Modern);

        press(HotkeyEvent::Press);
        press(HotkeyEvent::Release {
            elapsed_ms: LONG_PRESS_MS,
        });
        assert_eq!(target.overlay_mode.mode(), OverlayMode::Fps);
        assert_eq!(target.overlay_renderer.current(), OverlayRenderer::Ascii);
        assert_eq!(
            target
                .overlay_feedback
                .current(std::time::Instant::now())
                .hotkey,
            Some(Hotkey::F11)
        );

        press(HotkeyEvent::Release { elapsed_ms: 10 });
        assert_eq!(target.overlay_mode.mode(), OverlayMode::Off);
        press(HotkeyEvent::Release { elapsed_ms: 2000 });
        assert_eq!(target.overlay_mode.mode(), OverlayMode::Off);
        assert_eq!(target.overlay_renderer.current(), OverlayRenderer::Modern);

        dispatch_to_target(&target, Hotkey::F12, HotkeyEvent::Press);
        dispatch_to_target(
            &target,
            Hotkey::F12,
            HotkeyEvent::Release { elapsed_ms: 2000 },
        );
        assert_eq!(target.controller.target_fps(), Some(65.0));
        assert_eq!(target.overlay_renderer.current(), OverlayRenderer::Modern);
    }

    #[test]
    /// Checks cycling through every AUTO algorithm before OFF and manual restoration.
    fn f9_cycles_all_auto_algorithms_and_restores_the_target() {
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
