use crate::layer::auto::{AutoEvaluationSchedule, AutoRegulator};
use crate::layer::config::{Config, Hotkey};
use crate::layer::diagnostics;
use crate::layer::input_x11::HotkeyWorker;
use crate::layer::lib::{GpuMeasurement, HardwareMetrics, TelemetryStatus};
use crate::layer::limiter::{wait_until, FrameCapController, FrameScheduler};
use crate::layer::renderer::{
    OverlayFeedbackEvent, OverlayFeedbackState, OverlayMode, OverlayModeState,
    VulkanSwapchainRenderer,
};
use crate::layer::telemetry::{
    AmdGpuProvider, CpuUsageProvider, GpuTelemetryProvider, GpuUsageSmoother,
    HardwareMetricsSmoother, HwmonMetricsProvider,
};
use ash::vk::{self, Handle};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::ffi::{c_char, c_void, CStr};
use std::mem;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const LOADER_INSTANCE_CREATE_INFO: i32 = 47;
const LOADER_DEVICE_CREATE_INFO: i32 = 48;
const LAYER_LINK_INFO: i32 = 0;
type PfnGetPhysicalDeviceProcAddr =
    Option<unsafe extern "system" fn(vk::Instance, *const c_char) -> vk::PFN_vkVoidFunction>;

#[repr(C)]
struct LayerInstanceLink {
    next: *mut LayerInstanceLink,
    next_get_instance_proc_addr: vk::PFN_vkGetInstanceProcAddr,
    next_get_physical_device_proc_addr: PfnGetPhysicalDeviceProcAddr,
}

#[repr(C)]
union InstanceLayerInfoUnion {
    layer_info: *mut LayerInstanceLink,
    set_loader_data: *const c_void,
}

#[repr(C)]
struct LayerInstanceCreateInfo {
    s_type: i32,
    next: *const c_void,
    function: i32,
    u: InstanceLayerInfoUnion,
}

#[repr(C)]
struct LayerDeviceLink {
    next: *mut LayerDeviceLink,
    next_get_instance_proc_addr: vk::PFN_vkGetInstanceProcAddr,
    next_get_device_proc_addr: vk::PFN_vkGetDeviceProcAddr,
}

#[repr(C)]
union DeviceLayerInfoUnion {
    layer_info: *mut LayerDeviceLink,
    set_loader_data: *const c_void,
}

#[repr(C)]
struct LayerDeviceCreateInfo {
    s_type: i32,
    next: *const c_void,
    function: i32,
    u: DeviceLayerInfoUnion,
}

#[repr(C)]
pub struct NegotiateLayerInterface {
    s_type: i32,
    next: *mut c_void,
    loader_layer_interface_version: u32,
    get_instance_proc_addr: vk::PFN_vkGetInstanceProcAddr,
    get_device_proc_addr: vk::PFN_vkGetDeviceProcAddr,
    get_physical_device_proc_addr: PfnGetPhysicalDeviceProcAddr,
}

#[derive(Clone, Copy)]
struct InstanceDispatch {
    handle: vk::Instance,
    next_gipa: vk::PFN_vkGetInstanceProcAddr,
}

struct DeviceState {
    next_gdpa: vk::PFN_vkGetDeviceProcAddr,
    next_present: Option<vk::PFN_vkQueuePresentKHR>,
    ash_device: ash::Device,
    telemetry: TelemetryWorker,
    telemetry_expiry: Duration,
    fps_smoothing_window: Duration,
    minimum_metrics_collection_interval: Duration,
    overlay_refresh_rate: u64,
    scale_2x_min_width_px: u32,
    overlay_background_opacity: f32,
    overlay_fps_color: [u8; 3],
    queue_families: HashMap<usize, u32>,
    frame_cap: Arc<FrameCapController>,
    overlay_mode: Arc<OverlayModeState>,
    active_auto_algorithm: Arc<AtomicI32>,
    auto_hysteresis_percent: Arc<Vec<AtomicU32>>,
    hotkey_config: [Hotkey; 4],
    manual_step_fps: f32,
    overlay_feedback: Arc<OverlayFeedbackState>,
    auto_regulator: AutoRegulator,
    auto_evaluation_schedule: AutoEvaluationSchedule,
    schedulers: HashMap<usize, FrameScheduler>,
    hotkeys: Option<HotkeyWorker>,
    swapchains: HashMap<u64, VulkanSwapchainRenderer>,
}

struct TelemetryWorker {
    latest: Arc<parking_lot::RwLock<TelemetrySample>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

#[derive(Clone)]
struct TelemetrySample {
    gpu: GpuMeasurement,
    hardware: HardwareMetrics,
    hardware_timestamp: Instant,
}

impl TelemetryWorker {
    /// Starts polling and smoothing GPU usage on a background thread.
    fn start(
        provider: AmdGpuProvider,
        hwmon_provider: HwmonMetricsProvider,
        poll_rate: Duration,
        smoothing_window: Duration,
    ) -> Self {
        let initial = provider.get_metric();
        let mut smoother = GpuUsageSmoother::new(smoothing_window);
        let mut hardware_smoother = HardwareMetricsSmoother::new(smoothing_window);
        let mut cpu_usage = CpuUsageProvider::new();
        let mut initial_hardware = hwmon_provider.get_metrics();
        initial_hardware.cpu_load_percent = cpu_usage.get_load_percent();
        let initial_hardware_timestamp = Instant::now();
        let initial_hardware = hardware_smoother.add(initial_hardware, initial_hardware_timestamp);
        let latest = Arc::new(parking_lot::RwLock::new(TelemetrySample {
            gpu: smoother.add(initial),
            hardware: initial_hardware,
            hardware_timestamp: initial_hardware_timestamp,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let worker_latest = Arc::clone(&latest);
        let worker_stop = Arc::clone(&stop);
        let poll_rate = poll_rate.clamp(Duration::from_millis(10), Duration::from_secs(1));
        let worker = thread::spawn(move || {
            while !worker_stop.load(Ordering::Relaxed) {
                thread::sleep(poll_rate);
                if worker_stop.load(Ordering::Relaxed) {
                    break;
                }
                let mut hardware = hwmon_provider.get_metrics();
                hardware.cpu_load_percent = cpu_usage.get_load_percent();
                let hardware_timestamp = Instant::now();
                let hardware = hardware_smoother.add(hardware, hardware_timestamp);
                *worker_latest.write() = TelemetrySample {
                    gpu: smoother.add(provider.get_metric()),
                    hardware,
                    hardware_timestamp,
                };
            }
        });
        Self {
            latest,
            stop,
            worker: Some(worker),
        }
    }

    /// Returns the latest GPU sample, marking it expired after the configured age.
    fn current(&self, expiry: Duration) -> TelemetrySample {
        let mut sample = self.latest.read().clone();
        if sample.gpu.status == TelemetryStatus::Available
            && sample.gpu.timestamp.elapsed() > expiry
        {
            sample.gpu.status = TelemetryStatus::Expired;
            sample.gpu.value = 0.0;
        }
        sample
    }

    /// Stops and joins the GPU polling thread.
    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Returns the global map of Vulkan instance dispatch data.
fn instances() -> &'static Mutex<HashMap<usize, InstanceDispatch>> {
    static INSTANCES: OnceLock<Mutex<HashMap<usize, InstanceDispatch>>> = OnceLock::new();
    INSTANCES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Returns the map from enumerated physical devices to their parent instances.
fn physical_device_instances() -> &'static Mutex<HashMap<usize, usize>> {
    static PHYSICAL_DEVICE_INSTANCES: OnceLock<Mutex<HashMap<usize, usize>>> = OnceLock::new();
    PHYSICAL_DEVICE_INSTANCES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Returns the global map of tracked Vulkan devices.
fn devices() -> &'static Mutex<HashMap<usize, Arc<Mutex<DeviceState>>>> {
    static DEVICES: OnceLock<Mutex<HashMap<usize, Arc<Mutex<DeviceState>>>>> = OnceLock::new();
    DEVICES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Converts a Vulkan handle to its raw integer lookup key.
fn key<T: Handle>(handle: T) -> usize {
    handle.as_raw() as usize
}

/// Converts a queue handle to its device-state lookup key.
fn queue_key(queue: vk::Queue) -> usize {
    queue.as_raw() as usize
}

/// Finds the tracked device state that owns a queue.
fn device_for_queue(queue: vk::Queue) -> Option<Arc<Mutex<DeviceState>>> {
    let queue_dispatch_key = queue_key(queue);
    devices().lock().values().find_map(|state| {
        let contains_queue = state
            .lock()
            .queue_families
            .contains_key(&queue_dispatch_key);
        contains_queue.then(|| state.clone())
    })
}

/// Returns the dispatch-table key for a Vulkan handle.
unsafe fn dispatch_key<T: Handle>(handle: T) -> usize {
    key(handle)
}

/// Converts a non-null Vulkan procedure name to a C string reference.
unsafe fn proc_name(name: *const c_char) -> Option<&'static CStr> {
    (!name.is_null()).then(|| CStr::from_ptr(name))
}

/// Wraps a layer function pointer in Vulkan's optional function-pointer type.
unsafe fn proc<T: Copy>(function: T) -> vk::PFN_vkVoidFunction {
    Some(mem::transmute_copy(&function))
}

/// Resolves the next layer's instance proc-address function.
unsafe fn next_instance_gipa(instance: vk::Instance) -> Option<vk::PFN_vkGetInstanceProcAddr> {
    if instance.is_null() {
        return instances()
            .lock()
            .values()
            .next()
            .map(|state| state.next_gipa);
    }
    instances()
        .lock()
        .get(&key(instance))
        .map(|state| state.next_gipa)
}

/// Finds the instance dispatch data associated with a physical device.
fn instance_dispatch_for_physical_device(
    physical_device: vk::PhysicalDevice,
) -> Option<InstanceDispatch> {
    let physical_device_instances = physical_device_instances().lock();
    let tracked_instances = instances().lock();
    find_instance_dispatch(
        key(physical_device),
        &physical_device_instances,
        &tracked_instances,
    )
}

/// Selects a mapped parent instance or the safe single-instance fallback.
fn find_instance_dispatch(
    physical_device_key: usize,
    physical_device_instances: &HashMap<usize, usize>,
    instances: &HashMap<usize, InstanceDispatch>,
) -> Option<InstanceDispatch> {
    if let Some(instance_key) = physical_device_instances.get(&physical_device_key) {
        return instances.get(instance_key).copied();
    }
    if instances.len() == 1 {
        return instances.values().next().copied();
    }
    None
}

/// Records the parent instance for an enumerated physical device.
fn track_physical_device(instance: vk::Instance, physical_device: vk::PhysicalDevice) {
    physical_device_instances()
        .lock()
        .insert(key(physical_device), key(instance));
}

/// Calls downstream enumeration and records returned physical-device handles.
unsafe fn enumerate_physical_devices(
    instance: vk::Instance,
    count: *mut u32,
    physical_devices: *mut vk::PhysicalDevice,
    enumerate: vk::PFN_vkEnumeratePhysicalDevices,
) -> vk::Result {
    let result = enumerate(instance, count, physical_devices);
    if matches!(result, vk::Result::SUCCESS | vk::Result::INCOMPLETE)
        && !count.is_null()
        && !physical_devices.is_null()
    {
        let devices = std::slice::from_raw_parts(physical_devices, *count as usize);
        for &physical_device in devices {
            track_physical_device(instance, physical_device);
        }
    }
    result
}

/// Calls downstream group enumeration and records every returned device handle.
unsafe fn enumerate_physical_device_groups(
    instance: vk::Instance,
    count: *mut u32,
    groups: *mut vk::PhysicalDeviceGroupProperties<'_>,
    enumerate: vk::PFN_vkEnumeratePhysicalDeviceGroups,
) -> vk::Result {
    let result = enumerate(instance, count, groups);
    if matches!(result, vk::Result::SUCCESS | vk::Result::INCOMPLETE)
        && !count.is_null()
        && !groups.is_null()
    {
        let groups = std::slice::from_raw_parts(groups, *count as usize);
        for group in groups {
            for &physical_device in group
                .physical_devices
                .iter()
                .take(group.physical_device_count as usize)
            {
                track_physical_device(instance, physical_device);
            }
        }
    }
    result
}

#[no_mangle]
/// Negotiates the Vulkan loader interface version and layer entry points.
pub unsafe extern "system" fn vkNegotiateLoaderLayerInterfaceVersion(
    interface: *mut NegotiateLayerInterface,
) -> vk::Result {
    diagnostics::initialize();
    if interface.is_null() {
        diagnostics::error("vkNegotiateLoaderLayerInterfaceVersion received a null interface");
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    }
    let requested_version = (*interface).loader_layer_interface_version;
    (*interface).loader_layer_interface_version =
        (*interface).loader_layer_interface_version.min(2);
    (*interface).get_instance_proc_addr = vkGetInstanceProcAddr;
    (*interface).get_device_proc_addr = vkGetDeviceProcAddr;
    (*interface).get_physical_device_proc_addr = None;
    diagnostics::info(format!(
        "loader interface negotiated: requested_version={requested_version} selected_version={}",
        (*interface).loader_layer_interface_version
    ));
    vk::Result::SUCCESS
}

#[no_mangle]
/// Resolves Vulkan instance-level functions, intercepting Framecap entry points.
pub unsafe extern "system" fn vkGetInstanceProcAddr(
    instance: vk::Instance,
    name: *const c_char,
) -> vk::PFN_vkVoidFunction {
    let Some(name) = proc_name(name) else {
        return None;
    };
    match name.to_bytes() {
        b"vkGetInstanceProcAddr" => proc(vkGetInstanceProcAddr as vk::PFN_vkGetInstanceProcAddr),
        b"vkGetDeviceProcAddr" => proc(vkGetDeviceProcAddr as vk::PFN_vkGetDeviceProcAddr),
        b"vkCreateInstance" => proc(vkCreateInstance as vk::PFN_vkCreateInstance),
        b"vkDestroyInstance" => proc(vkDestroyInstance as vk::PFN_vkDestroyInstance),
        b"vkEnumeratePhysicalDevices" => {
            proc(vkEnumeratePhysicalDevices as vk::PFN_vkEnumeratePhysicalDevices)
        }
        b"vkEnumeratePhysicalDeviceGroups" => {
            proc(vkEnumeratePhysicalDeviceGroups as vk::PFN_vkEnumeratePhysicalDeviceGroups)
        }
        b"vkEnumeratePhysicalDeviceGroupsKHR" => {
            proc(vkEnumeratePhysicalDeviceGroupsKHR as vk::PFN_vkEnumeratePhysicalDeviceGroups)
        }
        b"vkCreateDevice" => proc(vkCreateDevice as vk::PFN_vkCreateDevice),
        _ => next_instance_gipa(instance).and_then(|next| next(instance, name.as_ptr())),
    }
}

#[no_mangle]
/// Enumerates physical devices and records their owning instance.
pub unsafe extern "system" fn vkEnumeratePhysicalDevices(
    instance: vk::Instance,
    count: *mut u32,
    physical_devices: *mut vk::PhysicalDevice,
) -> vk::Result {
    let Some(next_gipa) = next_instance_gipa(instance) else {
        diagnostics::error("vkEnumeratePhysicalDevices: instance is not tracked by Framecap");
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    let Some(enumerate) = next_gipa(instance, c"vkEnumeratePhysicalDevices".as_ptr()) else {
        diagnostics::error("vkEnumeratePhysicalDevices: downstream function was not found");
        return vk::Result::ERROR_EXTENSION_NOT_PRESENT;
    };
    let enumerate: vk::PFN_vkEnumeratePhysicalDevices = mem::transmute(enumerate);
    let result = enumerate_physical_devices(instance, count, physical_devices, enumerate);
    match (result, count.is_null()) {
        (vk::Result::SUCCESS | vk::Result::INCOMPLETE, false) => diagnostics::info(format!(
            "vkEnumeratePhysicalDevices completed: instance=0x{:x} result={result:?} count={} output_provided={}",
            key(instance),
            *count,
            !physical_devices.is_null()
        )),
        _ => diagnostics::warn(format!(
            "vkEnumeratePhysicalDevices returned: instance=0x{:x} result={result:?}",
            key(instance)
        )),
    }
    result
}

#[no_mangle]
/// Enumerates physical-device groups and records each group's device ownership.
pub unsafe extern "system" fn vkEnumeratePhysicalDeviceGroups(
    instance: vk::Instance,
    count: *mut u32,
    groups: *mut vk::PhysicalDeviceGroupProperties<'_>,
) -> vk::Result {
    let Some(next_gipa) = next_instance_gipa(instance) else {
        diagnostics::error("vkEnumeratePhysicalDeviceGroups: instance is not tracked by Framecap");
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    let Some(enumerate) = next_gipa(instance, c"vkEnumeratePhysicalDeviceGroups".as_ptr()) else {
        diagnostics::error("vkEnumeratePhysicalDeviceGroups: downstream function was not found");
        return vk::Result::ERROR_EXTENSION_NOT_PRESENT;
    };
    let enumerate: vk::PFN_vkEnumeratePhysicalDeviceGroups = mem::transmute(enumerate);
    let result = enumerate_physical_device_groups(instance, count, groups, enumerate);
    match (result, count.is_null()) {
        (vk::Result::SUCCESS | vk::Result::INCOMPLETE, false) => diagnostics::info(format!(
            "vkEnumeratePhysicalDeviceGroups completed: instance=0x{:x} result={result:?} groups={} output_provided={}",
            key(instance),
            *count,
            !groups.is_null()
        )),
        _ => diagnostics::warn(format!(
            "vkEnumeratePhysicalDeviceGroups returned: instance=0x{:x} result={result:?}",
            key(instance)
        )),
    }
    result
}

#[no_mangle]
/// KHR alias for physical-device-group enumeration.
pub unsafe extern "system" fn vkEnumeratePhysicalDeviceGroupsKHR(
    instance: vk::Instance,
    count: *mut u32,
    groups: *mut vk::PhysicalDeviceGroupProperties<'_>,
) -> vk::Result {
    let Some(next_gipa) = next_instance_gipa(instance) else {
        diagnostics::error(
            "vkEnumeratePhysicalDeviceGroupsKHR: instance is not tracked by Framecap",
        );
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    let Some(enumerate) = next_gipa(instance, c"vkEnumeratePhysicalDeviceGroupsKHR".as_ptr())
    else {
        diagnostics::error("vkEnumeratePhysicalDeviceGroupsKHR: downstream function was not found");
        return vk::Result::ERROR_EXTENSION_NOT_PRESENT;
    };
    let enumerate: vk::PFN_vkEnumeratePhysicalDeviceGroups = mem::transmute(enumerate);
    let result = enumerate_physical_device_groups(instance, count, groups, enumerate);
    match (result, count.is_null()) {
        (vk::Result::SUCCESS | vk::Result::INCOMPLETE, false) => diagnostics::info(format!(
            "vkEnumeratePhysicalDeviceGroupsKHR completed: instance=0x{:x} result={result:?} groups={} output_provided={}",
            key(instance),
            *count,
            !groups.is_null()
        )),
        _ => diagnostics::warn(format!(
            "vkEnumeratePhysicalDeviceGroupsKHR returned: instance=0x{:x} result={result:?}",
            key(instance)
        )),
    }
    result
}

#[no_mangle]
/// Resolves Vulkan device-level functions, intercepting Framecap entry points.
pub unsafe extern "system" fn vkGetDeviceProcAddr(
    device: vk::Device,
    name: *const c_char,
) -> vk::PFN_vkVoidFunction {
    let Some(name) = proc_name(name) else {
        return None;
    };
    match name.to_bytes() {
        b"vkGetDeviceProcAddr" => proc(vkGetDeviceProcAddr as vk::PFN_vkGetDeviceProcAddr),
        b"vkDestroyDevice" => proc(vkDestroyDevice as vk::PFN_vkDestroyDevice),
        b"vkQueuePresentKHR" => proc(vkQueuePresentKHR as vk::PFN_vkQueuePresentKHR),
        b"vkCreateSwapchainKHR" => proc(vkCreateSwapchainKHR as vk::PFN_vkCreateSwapchainKHR),
        b"vkDestroySwapchainKHR" => proc(vkDestroySwapchainKHR as vk::PFN_vkDestroySwapchainKHR),
        b"vkGetDeviceQueue" => proc(vkGetDeviceQueue as vk::PFN_vkGetDeviceQueue),
        b"vkGetDeviceQueue2" => proc(vkGetDeviceQueue2 as vk::PFN_vkGetDeviceQueue2),
        _ => devices()
            .lock()
            .get(&dispatch_key(device))
            .and_then(|state| {
                let next_gdpa = state.lock().next_gdpa;
                next_gdpa(device, name.as_ptr())
            }),
    }
}

#[no_mangle]
/// Creates and tracks a Vulkan instance while forwarding to the next layer.
pub unsafe extern "system" fn vkCreateInstance(
    create_info: *const vk::InstanceCreateInfo<'_>,
    allocator: *const vk::AllocationCallbacks<'_>,
    instance: *mut vk::Instance,
) -> vk::Result {
    if create_info.is_null() || instance.is_null() {
        diagnostics::error("vkCreateInstance received a null create_info or instance pointer");
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    }
    let mut chain = (*create_info).p_next;
    let mut next_gipa = None;
    let mut link_found = false;
    while !chain.is_null() {
        let header = chain.cast::<LayerInstanceCreateInfo>();
        if (*header).s_type == LOADER_INSTANCE_CREATE_INFO && (*header).function == LAYER_LINK_INFO
        {
            link_found = true;
            let link = (*header).u.layer_info;
            if link.is_null() {
                diagnostics::error("vkCreateInstance: loader instance link is null");
                return vk::Result::ERROR_INITIALIZATION_FAILED;
            }
            next_gipa = Some((*link).next_get_instance_proc_addr);
            (*header.cast_mut()).u.layer_info = (*link).next;
            break;
        }
        chain = (*(chain.cast::<vk::BaseInStructure<'_>>())).p_next.cast();
    }
    let Some(next_gipa) = next_gipa else {
        diagnostics::error(format!(
            "vkCreateInstance: loader instance link not found (found={link_found})"
        ));
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    let Some(create_instance) = next_gipa(vk::Instance::null(), c"vkCreateInstance".as_ptr())
    else {
        diagnostics::error("vkCreateInstance: downstream vkCreateInstance was not found");
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    let application = (*create_info).p_application_info;
    let application_name = if application.is_null() || (*application).p_application_name.is_null() {
        "<unspecified>".to_owned()
    } else {
        CStr::from_ptr((*application).p_application_name)
            .to_string_lossy()
            .into_owned()
    };
    let engine_name = if application.is_null() || (*application).p_engine_name.is_null() {
        "<unspecified>".to_owned()
    } else {
        CStr::from_ptr((*application).p_engine_name)
            .to_string_lossy()
            .into_owned()
    };
    diagnostics::info(format!(
        "vkCreateInstance requested: application={application_name:?} engine={engine_name:?} api_version={} enabled_extensions={} enabled_layers={}",
        if application.is_null() {
            "unspecified".to_owned()
        } else {
            format!(
                "{}.{}.{}",
                vk::api_version_major((*application).api_version),
                vk::api_version_minor((*application).api_version),
                vk::api_version_patch((*application).api_version)
            )
        },
        (*create_info).enabled_extension_count,
        (*create_info).enabled_layer_count,
    ));
    let create_instance: vk::PFN_vkCreateInstance = mem::transmute(create_instance);
    let start = Instant::now();
    let result = create_instance(create_info, allocator, instance);
    let elapsed = start.elapsed();
    if result != vk::Result::SUCCESS {
        diagnostics::error(format!(
            "downstream vkCreateInstance failed: result={result:?} elapsed_ms={:.3}",
            elapsed.as_secs_f64() * 1000.0
        ));
    }
    if result == vk::Result::SUCCESS {
        instances().lock().insert(
            key(*instance),
            InstanceDispatch {
                handle: *instance,
                next_gipa,
            },
        );
        diagnostics::info(format!(
            "vkCreateInstance succeeded: elapsed_ms={:.3}; Framecap Vulkan layer loaded",
            elapsed.as_secs_f64() * 1000.0
        ));
    }
    result
}

#[no_mangle]
/// Removes instance and physical-device tracking before forwarding destruction.
pub unsafe extern "system" fn vkDestroyInstance(
    instance: vk::Instance,
    allocator: *const vk::AllocationCallbacks<'_>,
) {
    let instance_key = key(instance);
    physical_device_instances()
        .lock()
        .retain(|_, parent_instance| *parent_instance != instance_key);
    if let Some(state) = instances().lock().remove(&instance_key) {
        if let Some(destroy) = (state.next_gipa)(instance, c"vkDestroyInstance".as_ptr()) {
            let destroy: vk::PFN_vkDestroyInstance = mem::transmute(destroy);
            destroy(instance, allocator);
        }
    }
}

#[no_mangle]
/// Creates a Vulkan device using the instance associated with its physical device.
pub unsafe extern "system" fn vkCreateDevice(
    physical_device: vk::PhysicalDevice,
    create_info: *const vk::DeviceCreateInfo<'_>,
    allocator: *const vk::AllocationCallbacks<'_>,
    device: *mut vk::Device,
) -> vk::Result {
    if create_info.is_null() || device.is_null() {
        diagnostics::error("vkCreateDevice received a null create_info or device pointer");
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    }
    let mut chain = (*create_info).p_next;
    let mut next_gdpa = None;
    while !chain.is_null() {
        let header = chain.cast::<LayerDeviceCreateInfo>();
        if (*header).s_type == LOADER_DEVICE_CREATE_INFO && (*header).function == LAYER_LINK_INFO {
            let link = (*header).u.layer_info;
            if link.is_null() {
                diagnostics::error("vkCreateDevice: loader device link is null");
                return vk::Result::ERROR_INITIALIZATION_FAILED;
            }
            next_gdpa = Some((*link).next_get_device_proc_addr);
            (*header.cast_mut()).u.layer_info = (*link).next;
            break;
        }
        chain = (*(chain.cast::<vk::BaseInStructure<'_>>())).p_next.cast();
    }
    let Some(next_gdpa) = next_gdpa else {
        diagnostics::error("vkCreateDevice: loader device link was not found");
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    let Some(instance_dispatch) = instance_dispatch_for_physical_device(physical_device) else {
        diagnostics::error(format!(
            "vkCreateDevice: no tracked instance found for physical device 0x{:x}",
            key(physical_device)
        ));
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    let Some(create_device) =
        (instance_dispatch.next_gipa)(instance_dispatch.handle, c"vkCreateDevice".as_ptr())
    else {
        diagnostics::error("vkCreateDevice: downstream vkCreateDevice was not found");
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    let create_device: vk::PFN_vkCreateDevice = mem::transmute(create_device);
    let start = Instant::now();
    let result = create_device(physical_device, create_info, allocator, device);
    let elapsed = start.elapsed();
    if result != vk::Result::SUCCESS {
        diagnostics::error(format!(
            "downstream vkCreateDevice failed: result={result:?} elapsed_ms={:.3}",
            elapsed.as_secs_f64() * 1000.0
        ));
    }
    if result == vk::Result::SUCCESS {
        let mut physical_properties = vk::PhysicalDeviceProperties::default();
        if let Some(get_properties) = (instance_dispatch.next_gipa)(
            instance_dispatch.handle,
            c"vkGetPhysicalDeviceProperties".as_ptr(),
        ) {
            let get_properties: vk::PFN_vkGetPhysicalDeviceProperties =
                mem::transmute(get_properties);
            get_properties(physical_device, &mut physical_properties);
        }
        let config = Config::load();
        let device_name = CStr::from_ptr(physical_properties.device_name.as_ptr())
            .to_string_lossy()
            .into_owned();
        diagnostics::info(format!(
            "vkCreateDevice succeeded: elapsed_ms={:.3} instance=0x{:x} physical_device=0x{:x} device={device_name:?} vendor_id=0x{:04x} device_id=0x{:04x} api_version={}.{}.{}",
            elapsed.as_secs_f64() * 1000.0,
            key(instance_dispatch.handle),
            key(physical_device),
            physical_properties.vendor_id,
            physical_properties.device_id,
            vk::api_version_major(physical_properties.api_version),
            vk::api_version_minor(physical_properties.api_version),
            vk::api_version_patch(physical_properties.api_version),
        ));
        let auto_algorithm_summary = config
            .auto_algorithms
            .iter()
            .map(|algorithm| {
                format!(
                    "{}(metric={:?},target={:?},hysteresis={}%,interval={}ms/{}ms)",
                    algorithm.name,
                    algorithm.metric,
                    algorithm.target,
                    algorithm.hysteresis_percent,
                    algorithm.increase_interval.as_millis(),
                    algorithm.decrease_interval.as_millis()
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        diagnostics::info(format!(
            "configuration: target_fps={} input_enabled={} hotkeys=auto:{:?},down:{:?},up:{:?},toggle:{:?} overlay_startup={} auto_algorithms=[{}] gpu_increase_limit={} gpu_decrease_limit={} frame_jitter_limit_ms={} manual_step_fps={} manual_bounds={}..{} gpu_poll_rate_ms={} overlay_refresh_rate={} gpu_expiry_ms={} gpu_smoothing_window_ms={} background_opacity={} fps_color={:?} window_size={}",
            config.target_fps.map_or_else(|| "OFF".to_owned(), |fps| fps.to_string()),
            config.input_enabled,
            config.hotkey_auto,
            config.hotkey_down,
            config.hotkey_up,
            config.hotkey_toggle,
            config.startup,
            auto_algorithm_summary,
            config.gpu_increase_limit,
            config.gpu_decrease_limit,
            config.frame_jitter_limit_ms,
            config.manual_step_fps,
            config.manual_min_fps,
            config.manual_max_fps,
            config.gpu_poll_rate_ms,
            config.refresh_rate,
            config.gpu_expiry_ms,
            config.gpu_smoothing_window_ms,
            config.overlay_background_opacity,
            config.overlay_fps_color,
            config.window_size,
        ));
        let frame_cap = Arc::new(FrameCapController::with_manual_limits(
            config.target_fps,
            config.manual_min_fps,
            config.manual_max_fps,
        ));
        let overlay_mode = Arc::new(OverlayModeState::new(config.startup));
        let active_auto_algorithm = Arc::new(AtomicI32::new(if config.target_fps.is_some() {
            0
        } else {
            -1
        }));
        let overlay_feedback = Arc::new(OverlayFeedbackState::new());
        let auto_hysteresis_percent = Arc::new(
            config
                .auto_algorithms
                .iter()
                .map(|algorithm| AtomicU32::new(algorithm.hysteresis_percent.to_bits()))
                .collect::<Vec<_>>(),
        );
        if config.input_enabled {
            diagnostics::info("registering device with process-wide X11/XWayland hotkey listener");
        } else {
            diagnostics::info("global hotkeys disabled by configuration");
        }
        let hotkeys = HotkeyWorker::start(
            config.input_enabled,
            config.hotkey_auto,
            config.hotkey_up,
            config.hotkey_down,
            config.hotkey_toggle,
            Arc::clone(&frame_cap),
            Arc::clone(&overlay_mode),
            Arc::clone(&active_auto_algorithm),
            Arc::clone(&auto_hysteresis_percent),
            config.manual_step_fps,
            Arc::clone(&overlay_feedback),
        );
        let next_present = next_gdpa(*device, c"vkQueuePresentKHR".as_ptr())
            .map(|function| mem::transmute(function));
        let ash_device = ash::Device::load_with(
            |name: &CStr| {
                next_gdpa(*device, name.as_ptr())
                    .map_or(std::ptr::null(), |function| mem::transmute(function))
            },
            *device,
        );
        let gpu_provider = AmdGpuProvider::for_device(
            physical_properties.vendor_id,
            physical_properties.device_id,
        );
        diagnostics::info(format!(
            "GPU telemetry provider selected: sysfs_path={:?}",
            gpu_provider.card_path
        ));
        devices().lock().insert(
            dispatch_key(*device),
            Arc::new(Mutex::new(DeviceState {
                next_gdpa,
                next_present,
                ash_device,
                telemetry: TelemetryWorker::start(
                    gpu_provider,
                    HwmonMetricsProvider::with_paths(
                        config.cpu_temp_path.clone(),
                        config.gpu_vrm_temp_path.clone(),
                        config.cpu_vrm_temp_path.clone(),
                        config.gpu_edge_temp_path.clone(),
                        config.gpu_power_path.clone(),
                        config.cpu_power_path.clone(),
                        config.ppt_power_path.clone(),
                        config.fan_rpm_path.clone(),
                    ),
                    Duration::from_millis(config.gpu_poll_rate_ms),
                    Duration::from_millis(config.gpu_smoothing_window_ms),
                ),
                telemetry_expiry: Duration::from_millis(config.gpu_expiry_ms),
                fps_smoothing_window: Duration::from_millis(config.gpu_smoothing_window_ms),
                minimum_metrics_collection_interval: Duration::from_millis(config.gpu_poll_rate_ms),
                overlay_refresh_rate: config.refresh_rate,
                scale_2x_min_width_px: config.scale_2x_min_width_px,
                overlay_background_opacity: config.overlay_background_opacity,
                overlay_fps_color: config.overlay_fps_color,
                auto_regulator: AutoRegulator::new(
                    config.auto_algorithms.clone(),
                    config.gpu_increase_limit,
                    config.gpu_decrease_limit,
                    config.frame_jitter_limit_ms,
                ),
                auto_evaluation_schedule: AutoEvaluationSchedule::new(Duration::from_millis(
                    config.gpu_poll_rate_ms,
                )),
                queue_families: HashMap::new(),
                frame_cap,
                overlay_mode,
                active_auto_algorithm,
                auto_hysteresis_percent,
                hotkey_config: [
                    config.hotkey_auto,
                    config.hotkey_up,
                    config.hotkey_down,
                    config.hotkey_toggle,
                ],
                manual_step_fps: config.manual_step_fps,
                overlay_feedback,
                schedulers: HashMap::new(),
                hotkeys,
                swapchains: HashMap::new(),
            })),
        );
    }
    result
}

#[no_mangle]
/// Creates a swapchain and initializes Framecap resources when its usage permits.
pub unsafe extern "system" fn vkCreateSwapchainKHR(
    device: vk::Device,
    create_info: *const vk::SwapchainCreateInfoKHR<'_>,
    allocator: *const vk::AllocationCallbacks<'_>,
    swapchain: *mut vk::SwapchainKHR,
) -> vk::Result {
    if create_info.is_null() || swapchain.is_null() {
        diagnostics::error("vkCreateSwapchainKHR received a null create_info or swapchain pointer");
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    }
    let device_key = dispatch_key(device);
    let Some(state) = devices().lock().get(&device_key).cloned() else {
        diagnostics::error("vkCreateSwapchainKHR: device is not tracked by Framecap");
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    let (
        next_gdpa,
        ash_device,
        fps_smoothing_window,
        minimum_metrics_collection_interval,
        overlay_refresh_rate,
        scale_2x_min_width_px,
        overlay_background_opacity,
        overlay_fps_color,
    ) = {
        let state = state.lock();
        (
            state.next_gdpa,
            state.ash_device.clone(),
            state.fps_smoothing_window,
            state.minimum_metrics_collection_interval,
            state.overlay_refresh_rate,
            state.scale_2x_min_width_px,
            state.overlay_background_opacity,
            state.overlay_fps_color,
        )
    };
    let create_swapchain = next_gdpa(device, c"vkCreateSwapchainKHR".as_ptr())
        .map(|function| mem::transmute::<_, vk::PFN_vkCreateSwapchainKHR>(function));
    let Some(create_swapchain) = create_swapchain else {
        diagnostics::error("vkCreateSwapchainKHR: downstream function was not found");
        return vk::Result::ERROR_EXTENSION_NOT_PRESENT;
    };
    let start = Instant::now();
    let result = create_swapchain(device, create_info, allocator, swapchain);
    let elapsed = start.elapsed();
    if result != vk::Result::SUCCESS {
        diagnostics::error(format!(
            "downstream vkCreateSwapchainKHR failed: result={result:?} extent={}x{} format={:?} image_usage={:?} elapsed_ms={:.3}",
            (*create_info).image_extent.width,
            (*create_info).image_extent.height,
            (*create_info).image_format,
            (*create_info).image_usage,
            elapsed.as_secs_f64() * 1000.0
        ));
    } else {
        diagnostics::info(format!(
            "vkCreateSwapchainKHR succeeded: extent={}x{} format={:?} image_usage={:?} elapsed_ms={:.3}",
            (*create_info).image_extent.width,
            (*create_info).image_extent.height,
            (*create_info).image_format,
            (*create_info).image_usage,
            elapsed.as_secs_f64() * 1000.0
        ));
    }
    if result != vk::Result::SUCCESS
        || !(*create_info)
            .image_usage
            .contains(vk::ImageUsageFlags::COLOR_ATTACHMENT)
    {
        if result == vk::Result::SUCCESS {
            diagnostics::warn(format!(
                "overlay renderer skipped: swapchain image usage {:?} does not include COLOR_ATTACHMENT",
                (*create_info).image_usage
            ));
        }
        return result;
    }

    let get_images = next_gdpa(device, c"vkGetSwapchainImagesKHR".as_ptr())
        .map(|function| mem::transmute::<_, vk::PFN_vkGetSwapchainImagesKHR>(function));
    let Some(get_images) = get_images else {
        diagnostics::error(
            "vkCreateSwapchainKHR: downstream vkGetSwapchainImagesKHR was not found",
        );
        return result;
    };
    let mut image_count = 0;
    let image_count_result = get_images(device, *swapchain, &mut image_count, std::ptr::null_mut());
    if image_count_result != vk::Result::SUCCESS {
        diagnostics::error(format!(
            "vkGetSwapchainImagesKHR failed while querying image count: result={image_count_result:?}"
        ));
        return result;
    }
    let mut images = vec![vk::Image::null(); image_count as usize];
    let image_result = get_images(device, *swapchain, &mut image_count, images.as_mut_ptr());
    if image_result != vk::Result::SUCCESS {
        diagnostics::error(format!(
            "vkGetSwapchainImagesKHR failed while retrieving images: result={image_result:?}"
        ));
        return result;
    }
    images.truncate(image_count as usize);
    match VulkanSwapchainRenderer::new(
        &ash_device,
        (*create_info).image_format,
        (*create_info).image_extent,
        &images,
        fps_smoothing_window,
        minimum_metrics_collection_interval,
        overlay_refresh_rate,
        overlay_background_opacity,
        overlay_fps_color,
        scale_2x_min_width_px,
    ) {
        Ok(renderer) => {
            state
                .lock()
                .swapchains
                .insert((*swapchain).as_raw(), renderer);
            diagnostics::info(format!(
                "overlay renderer initialized: images={} extent={}x{}",
                images.len(),
                (*create_info).image_extent.width,
                (*create_info).image_extent.height
            ));
        }
        Err(error) => diagnostics::error(format!(
            "overlay renderer initialization failed: result={error:?}; game presentation will continue without this renderer"
        )),
    }
    result
}

#[no_mangle]
/// Destroys Framecap swapchain resources before forwarding destruction.
pub unsafe extern "system" fn vkDestroySwapchainKHR(
    device: vk::Device,
    swapchain: vk::SwapchainKHR,
    allocator: *const vk::AllocationCallbacks<'_>,
) {
    if let Some(state) = devices().lock().get(&dispatch_key(device)).cloned() {
        let (next_gdpa, ash_device, mut renderer) = {
            let mut state = state.lock();
            (
                state.next_gdpa,
                state.ash_device.clone(),
                state.swapchains.remove(&swapchain.as_raw()),
            )
        };
        if let Some(renderer) = renderer.as_mut() {
            renderer.destroy(&ash_device);
        }
        let destroy_swapchain = next_gdpa(device, c"vkDestroySwapchainKHR".as_ptr())
            .map(|function| mem::transmute::<_, vk::PFN_vkDestroySwapchainKHR>(function));
        if let Some(destroy_swapchain) = destroy_swapchain {
            destroy_swapchain(device, swapchain, allocator);
        }
    }
}

#[no_mangle]
/// Retrieves and records a queue returned by the Vulkan device.
pub unsafe extern "system" fn vkGetDeviceQueue(
    device: vk::Device,
    queue_family_index: u32,
    queue_index: u32,
    queue: *mut vk::Queue,
) {
    if queue.is_null() {
        return;
    }
    let Some(state) = devices().lock().get(&dispatch_key(device)).cloned() else {
        return;
    };
    let next_gdpa = state.lock().next_gdpa;
    let get_queue = next_gdpa(device, c"vkGetDeviceQueue".as_ptr())
        .map(|function| mem::transmute::<_, vk::PFN_vkGetDeviceQueue>(function));
    if let Some(get_queue) = get_queue {
        get_queue(device, queue_family_index, queue_index, queue);
        if !(*queue).is_null() {
            state
                .lock()
                .queue_families
                .insert(dispatch_key(*queue), queue_family_index);
        }
    }
}

#[no_mangle]
/// Retrieves and records a queue using the Vulkan 1.1 queue-info structure.
pub unsafe extern "system" fn vkGetDeviceQueue2(
    device: vk::Device,
    queue_info: *const vk::DeviceQueueInfo2<'_>,
    queue: *mut vk::Queue,
) {
    if queue_info.is_null() || queue.is_null() {
        return;
    }
    let Some(state) = devices().lock().get(&dispatch_key(device)).cloned() else {
        return;
    };
    let next_gdpa = state.lock().next_gdpa;
    let get_queue = next_gdpa(device, c"vkGetDeviceQueue2".as_ptr())
        .map(|function| mem::transmute::<_, vk::PFN_vkGetDeviceQueue2>(function));
    if let Some(get_queue) = get_queue {
        get_queue(device, queue_info, queue);
        if !(*queue).is_null() {
            state
                .lock()
                .queue_families
                .insert(dispatch_key(*queue), (*queue_info).queue_family_index);
        }
    }
}

#[no_mangle]
/// Stops device workers, destroys layer resources, and forwards device destruction.
pub unsafe extern "system" fn vkDestroyDevice(
    device: vk::Device,
    allocator: *const vk::AllocationCallbacks<'_>,
) {
    if let Some(state) = devices().lock().remove(&dispatch_key(device)) {
        let (next_gdpa, ash_device, mut swapchains, mut hotkeys) = {
            let mut state = state.lock();
            (
                state.next_gdpa,
                state.ash_device.clone(),
                mem::take(&mut state.swapchains),
                state.hotkeys.take(),
            )
        };
        if let Some(hotkeys) = hotkeys.as_mut() {
            hotkeys.stop();
        }
        state.lock().telemetry.stop();
        for renderer in swapchains.values_mut() {
            renderer.destroy(&ash_device);
        }
        if let Some(destroy) = next_gdpa(device, c"vkDestroyDevice".as_ptr()) {
            let destroy: vk::PFN_vkDestroyDevice = mem::transmute(destroy);
            destroy(device, allocator);
        }
    }
}

#[no_mangle]
/// Applies frame pacing, updates metrics, renders the optional overlay, then presents.
pub unsafe extern "system" fn vkQueuePresentKHR(
    queue: vk::Queue,
    present_info: *const vk::PresentInfoKHR<'_>,
) -> vk::Result {
    if present_info.is_null() {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    }
    let Some(state) = device_for_queue(queue) else {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    let (present, ash_device, queue_family, metrics) = {
        let state = state.lock();
        (
            state.next_present,
            state.ash_device.clone(),
            state.queue_families.get(&queue_key(queue)).copied(),
            state.telemetry.current(state.telemetry_expiry),
        )
    };
    let Some(present) = present else {
        return vk::Result::ERROR_EXTENSION_NOT_PRESENT;
    };
    let deadline = {
        let mut state = state.lock();
        let target_fps = state.frame_cap.target_fps();
        let deadline = state
            .schedulers
            .entry(queue_key(queue))
            .or_default()
            .next_deadline(std::time::Instant::now(), target_fps);
        deadline
    };
    if let Some(deadline) = deadline {
        if let Err(wait_error) = wait_until(deadline) {
            static WAIT_ERROR_LOGGED: AtomicBool = AtomicBool::new(false);
            if !WAIT_ERROR_LOGGED.swap(true, Ordering::Relaxed) {
                diagnostics::error(format!(
                    "frame cap wait failed; continuing presentation: {wait_error}"
                ));
            }
        }
    }
    let info = &*present_info;
    if info.swapchain_count == 0 || info.p_swapchains.is_null() || info.p_image_indices.is_null() {
        return present(queue, present_info);
    }
    let swapchains = std::slice::from_raw_parts(info.p_swapchains, info.swapchain_count as usize);
    let image_indices =
        std::slice::from_raw_parts(info.p_image_indices, info.swapchain_count as usize);
    let waits = if info.wait_semaphore_count == 0 || info.p_wait_semaphores.is_null() {
        Vec::new()
    } else {
        std::slice::from_raw_parts(info.p_wait_semaphores, info.wait_semaphore_count as usize)
            .to_vec()
    };
    let mut commands = Vec::new();
    let mut signals = Vec::new();
    if let Some(queue_family) = queue_family {
        let mut state = state.lock();
        let mut cap_generation = state.frame_cap.generation();
        let active_mode = state.active_auto_algorithm.load(Ordering::Relaxed);
        let auto_algorithm_index = (active_mode > 0).then_some((active_mode - 1) as usize);
        let auto_algorithm_label =
            auto_algorithm_index.and_then(|index| state.auto_regulator.overlay_label(index));
        let auto_target =
            auto_algorithm_index.and_then(|index| state.auto_regulator.overlay_target(index));
        let mut target_fps = state.frame_cap.target_fps();
        let hysteresis_percent = auto_algorithm_index
            .and_then(|index| state.auto_hysteresis_percent.get(index))
            .map(|value| f32::from_bits(value.load(Ordering::Relaxed)));
        let frame_cap = Arc::clone(&state.frame_cap);
        let overlay_mode = state.overlay_mode.mode();
        for (&swapchain, &image_index) in swapchains.iter().zip(image_indices) {
            let Some((metrics, displayed_metrics)) = state
                .swapchains
                .get_mut(&swapchain.as_raw())
                .map(|renderer| {
                    renderer.record_present(
                        metrics.gpu.clone(),
                        metrics.hardware.clone(),
                        metrics.hardware_timestamp,
                        cap_generation,
                    )
                })
            else {
                continue;
            };
            let evaluation_time = Instant::now();
            let evaluation_due = state
                .auto_evaluation_schedule
                .should_evaluate(evaluation_time, auto_algorithm_index);
            if let Some(algorithm_index) = auto_algorithm_index.filter(|_| evaluation_due) {
                if let Some((old, new)) = state.auto_regulator.evaluate(
                    algorithm_index,
                    &metrics,
                    evaluation_time,
                    hysteresis_percent.unwrap_or_default(),
                    &frame_cap,
                ) {
                    state
                        .overlay_feedback
                        .record(OverlayFeedbackEvent::CapChanged);
                    diagnostics::info(format!(
                        "[AUTO] FPS limit adjusted: {old:.1} -> {new:.1} fps"
                    ));
                    target_fps = state.frame_cap.target_fps();
                    cap_generation = state.frame_cap.generation();
                }
            }
            if overlay_mode != OverlayMode::Off {
                let hotkey_config = state.hotkey_config;
                let manual_step_fps = state.manual_step_fps;
                let feedback = state.overlay_feedback.current(Instant::now());
                let Some(renderer) = state.swapchains.get_mut(&swapchain.as_raw()) else {
                    continue;
                };
                match renderer.record_commands(
                    &ash_device,
                    queue_family,
                    image_index,
                    &displayed_metrics,
                    overlay_mode,
                    target_fps,
                    auto_algorithm_index,
                    auto_algorithm_label,
                    hysteresis_percent,
                    auto_target,
                    hotkey_config,
                    manual_step_fps,
                    feedback,
                ) {
                    Ok((command, signal)) => {
                        commands.push(command);
                        signals.push(signal);
                    }
                    Err(error) => {
                        static RENDER_ERROR_LOGGED: AtomicBool = AtomicBool::new(false);
                        if !RENDER_ERROR_LOGGED.swap(true, Ordering::Relaxed) {
                            diagnostics::error(format!(
                                "overlay command recording failed: result={error:?}; subsequent render errors suppressed"
                            ));
                        }
                    }
                }
            }
        }
    }
    if commands.is_empty() {
        return present(queue, present_info);
    }

    let wait_stages = vec![vk::PipelineStageFlags::ALL_COMMANDS; waits.len()];
    let submit = [vk::SubmitInfo::default()
        .wait_semaphores(&waits)
        .wait_dst_stage_mask(&wait_stages)
        .command_buffers(&commands)
        .signal_semaphores(&signals)];
    if let Err(error) = ash_device.queue_submit(queue, &submit, vk::Fence::null()) {
        static SUBMIT_ERROR_LOGGED: AtomicBool = AtomicBool::new(false);
        if !SUBMIT_ERROR_LOGGED.swap(true, Ordering::Relaxed) {
            diagnostics::error(format!(
                "overlay queue submission failed: result={error:?}; subsequent submission errors suppressed"
            ));
        }
        return present(queue, present_info);
    }
    let mut modified_info = *info;
    modified_info.p_wait_semaphores = signals.as_ptr();
    modified_info.wait_semaphore_count = signals.len() as u32;
    present(queue, &modified_info)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Supplies a test dispatch function that reports no downstream entry points.
    unsafe extern "system" fn test_gipa(
        _instance: vk::Instance,
        _name: *const c_char,
    ) -> vk::PFN_vkVoidFunction {
        None
    }

    #[test]
    /// Checks that queue lookup keys preserve the raw Vulkan handle value.
    fn queue_handle_key_uses_raw_handle_value() {
        let queue = vk::Queue::from_raw(0x1234_5678_9abc_def0);
        assert_eq!(queue_key(queue), 0x1234_5678_9abc_def0usize);
    }

    #[test]
    /// Checks that a physical device resolves to its enumerating instance.
    fn physical_device_selects_its_parent_instance() {
        let first = vk::Instance::from_raw(1);
        let second = vk::Instance::from_raw(2);
        let physical_device = vk::PhysicalDevice::from_raw(3);
        let mut instances = HashMap::new();
        instances.insert(
            key(first),
            InstanceDispatch {
                handle: first,
                next_gipa: test_gipa,
            },
        );
        instances.insert(
            key(second),
            InstanceDispatch {
                handle: second,
                next_gipa: test_gipa,
            },
        );
        let physical_device_instances = HashMap::from([(key(physical_device), key(second))]);

        assert_eq!(
            find_instance_dispatch(key(physical_device), &physical_device_instances, &instances)
                .map(|dispatch| dispatch.handle),
            Some(second)
        );
    }

    #[test]
    /// Checks that fallback routing is allowed only for a single instance.
    fn unmapped_physical_device_only_falls_back_when_one_instance_exists() {
        let instance = vk::Instance::from_raw(1);
        let physical_device = vk::PhysicalDevice::from_raw(3);
        let instances = HashMap::from([(
            key(instance),
            InstanceDispatch {
                handle: instance,
                next_gipa: test_gipa,
            },
        )]);
        let physical_device_instances = HashMap::new();

        assert_eq!(
            find_instance_dispatch(key(physical_device), &physical_device_instances, &instances)
                .map(|dispatch| dispatch.handle),
            Some(instance)
        );

        let multiple_instances = HashMap::from([
            (
                key(instance),
                InstanceDispatch {
                    handle: instance,
                    next_gipa: test_gipa,
                },
            ),
            (
                key(vk::Instance::from_raw(2)),
                InstanceDispatch {
                    handle: vk::Instance::from_raw(2),
                    next_gipa: test_gipa,
                },
            ),
        ]);
        assert!(find_instance_dispatch(
            key(physical_device),
            &physical_device_instances,
            &multiple_instances
        )
        .is_none());
    }
}
