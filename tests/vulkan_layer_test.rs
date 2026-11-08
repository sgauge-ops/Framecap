use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static NEXT_LOG_ID: AtomicU64 = AtomicU64::new(0);

/// Runs `vkcube` with the requested Vulkan layer configuration and timeout.
fn run_vkcube(
    root: &Path,
    wsi: &str,
    layer_enabled: bool,
    target_fps: Option<&str>,
    overlay_startup: u8,
    frame_count: u32,
    immediate: bool,
) -> (Output, Duration, String) {
    let frame_count = frame_count.to_string();
    let log_path = env::temp_dir().join(format!(
        "framecap-vkcube-{}-{}.log",
        std::process::id(),
        NEXT_LOG_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let config_dir = env::temp_dir().join(format!(
        "framecap-config-{}-{}",
        std::process::id(),
        NEXT_LOG_ID.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&config_dir).expect("failed to create temporary Framecap config directory");
    let target_fps = target_fps.unwrap_or("60");
    fs::write(
        config_dir.join("framecap.toml"),
        format!(
            "[framecap]\ntarget_fps = {target_fps}\ninput_enabled = false\n\n[overlay]\nstartup = {overlay_startup}\n\n[diagnostics]\nlog_path = {:?}\n",
            log_path.display().to_string()
        ),
    )
    .expect("failed to write temporary Framecap config");
    let mut command = Command::new("timeout");
    command
        .args(["20s", "vkcube", "--c", &frame_count, "--wsi", wsi])
        .current_dir(&config_dir)
        .env("VK_LAYER_PATH", root.join("layer"))
        .env_remove("VK_INSTANCE_LAYERS");
    if layer_enabled {
        command.env("VK_INSTANCE_LAYERS", "VK_LAYER_FRAMECAP_observability");
    }
    if immediate {
        command.args(["--present_mode", "0"]);
    }
    let start = Instant::now();
    let output = command.output().expect("failed to run timeout/vkcube");
    let diagnostic_log = fs::read_to_string(&log_path).unwrap_or_default();
    if log_path.exists() {
        fs::remove_file(&log_path).expect("failed to remove temporary Framecap log");
    }
    fs::remove_file(config_dir.join("framecap.toml"))
        .expect("failed to remove temporary Framecap config");
    fs::remove_dir(&config_dir).expect("failed to remove temporary config directory");
    (output, start.elapsed(), diagnostic_log)
}

#[test]
/// Checks that `vkcube` presents frames both with and without Framecap.
fn vkcube_presents_with_and_without_framecap_layer() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let wsi = if env::var_os("WAYLAND_DISPLAY").is_some() {
        "wayland"
    } else if env::var_os("DISPLAY").is_some() {
        "xcb"
    } else {
        eprintln!("Skipping vkcube integration test: no graphical display available");
        return;
    };

    if Command::new("vkcube").arg("--help").output().is_err()
        || Command::new("timeout").arg("--version").output().is_err()
        || !root.join("target/debug/libframecap.so").is_file()
    {
        eprintln!("Skipping vkcube integration test: vkcube, timeout, or layer artifact missing");
        return;
    }

    for layer_enabled in [false, true] {
        let (output, _, diagnostic_log) = run_vkcube(root, wsi, layer_enabled, None, 2, 60, false);
        let log = format!(
            "{}\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
            diagnostic_log
        );
        assert!(
            output.status.success(),
            "vkcube failed (layer_enabled={layer_enabled}):\n{log}"
        );
        assert_eq!(
            log.contains("Framecap Vulkan layer loaded"),
            layer_enabled,
            "unexpected Framecap loader status (layer_enabled={layer_enabled}):\n{log}"
        );
        if layer_enabled {
            for entry in [
                "Framecap diagnostic session started",
                "vkCreateInstance requested",
                "vkCreateDevice succeeded",
                "configuration:",
            ] {
                assert!(
                    diagnostic_log.contains(entry),
                    "diagnostic log is missing {entry:?}:\n{diagnostic_log}"
                );
            }
        }
    }

    for mode in 0..=2 {
        let (output, _, diagnostic_log) = run_vkcube(root, wsi, true, None, mode, 20, false);
        assert!(output.status.success(), "mode {mode} failed: {output:?}");
        assert!(
            diagnostic_log.contains(&format!("overlay_startup={mode}")),
            "mode {mode} was not loaded:\n{diagnostic_log}"
        );
        assert!(
            diagnostic_log.contains("overlay renderer initialized"),
            "mode {mode} did not initialize a renderer:\n{diagnostic_log}"
        );
    }

    let (uncapped_output, uncapped_time, _) = run_vkcube(root, wsi, true, Some("0"), 2, 30, true);
    if !uncapped_output.status.success() {
        eprintln!("Skipping frame-cap timing check: immediate present mode is unavailable");
        return;
    }
    let (capped_output, capped_time, diagnostic_log) =
        run_vkcube(root, wsi, true, Some("30"), 2, 30, true);
    let capped_log = format!(
        "{}\n{}\n{}",
        String::from_utf8_lossy(&capped_output.stdout),
        String::from_utf8_lossy(&capped_output.stderr),
        diagnostic_log
    );
    assert!(
        capped_output.status.success(),
        "capped vkcube failed:\n{capped_log}"
    );
    assert!(
        capped_time >= Duration::from_millis(700),
        "30 FPS cap completed 30 frames too quickly: {capped_time:?}"
    );
    assert!(
        capped_time > uncapped_time + Duration::from_millis(400),
        "frame cap did not measurably slow presentation: uncapped={uncapped_time:?}, capped={capped_time:?}"
    );
}
