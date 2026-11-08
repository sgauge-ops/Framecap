use super::*;
use fontdue::{Font, FontSettings, Metrics};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const WIDTH: u32 = 415;
const HEIGHT: u32 = 260;
const FPS_VALUE_WIDTH: u32 = 94;
const FPS_LABEL_GAP: u32 = 6;
const LABEL: usize = 0;
const VALUE: usize = 1;
const ACCENT: usize = 2;
const FEEDBACK: usize = 3;
const LINE: usize = 4;
const MODE: usize = 5;
const COVERAGE_LEVELS: usize = 8;
const RECTANGLES_PER_DRAW: usize = 6;

struct Glyph {
    metrics: Metrics,
    spans: Vec<(u32, u32, u32, usize)>,
}

struct FontCache {
    regular: Font,
    bold: Font,
    glyphs: HashMap<(char, u32, bool), Glyph>,
}

impl FontCache {
    fn new() -> Self {
        Self {
            regular: Font::from_bytes(
                include_bytes!("fonts/FiraSans-Regular.ttf") as &[u8],
                FontSettings::default(),
            )
            .expect("bundled regular font must be valid"),
            bold: Font::from_bytes(
                include_bytes!("fonts/FiraSans-Bold.ttf") as &[u8],
                FontSettings::default(),
            )
            .expect("bundled bold font must be valid"),
            glyphs: HashMap::new(),
        }
    }

    fn glyph(&mut self, character: char, size: u32, bold: bool) -> &Glyph {
        self.glyphs
            .entry((character, size, bold))
            .or_insert_with(|| {
                let font = if bold { &self.bold } else { &self.regular };
                let (metrics, bitmap) = font.rasterize(character, size as f32);
                let mut spans = Vec::new();
                for y in 0..metrics.height {
                    let mut x = 0;
                    while x < metrics.width {
                        let coverage =
                            (usize::from(bitmap[y * metrics.width + x]) * COVERAGE_LEVELS + 127)
                                / 255;
                        if coverage == 0 {
                            x += 1;
                            continue;
                        }
                        let start = x;
                        while x < metrics.width
                            && (usize::from(bitmap[y * metrics.width + x]) * COVERAGE_LEVELS + 127)
                                / 255
                                == coverage
                        {
                            x += 1;
                        }
                        spans.push((start as u32, y as u32, (x - start) as u32, coverage));
                    }
                }
                Glyph { metrics, spans }
            })
    }
}

static FONTS: OnceLock<Mutex<FontCache>> = OnceLock::new();

pub(super) struct Dashboard {
    pub width: u32,
    pub height: u32,
    scale: u32,
    pub groups: [Vec<OverlayRect>; 6],
    edges: [[Vec<OverlayRect>; COVERAGE_LEVELS - 1]; 6],
    boxes: Vec<DashboardBox>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BoxStyle {
    Mode,
    Key,
    ActiveKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DashboardBox {
    rect: OverlayRect,
    style: BoxStyle,
}

#[derive(PartialEq)]
pub(super) struct DashboardKey {
    pub timestamp: Instant,
    pub mode: OverlayMode,
    pub target_fps: Option<f32>,
    pub auto_algorithm_index: Option<usize>,
    pub auto_algorithm_label: Option<String>,
    pub hysteresis_percent: Option<f32>,
    pub auto_target: Option<(AutoMetric, AutoTarget)>,
    pub hotkeys: [Hotkey; 4],
    pub manual_step_fps: f32,
    pub feedback: OverlayFeedback,
}

pub(super) struct PreparedDashboard {
    width: u32,
    height: u32,
    scale: u32,
    groups: [Vec<vk::ClearRect>; 6],
    edges: [[Vec<OverlayRect>; COVERAGE_LEVELS - 1]; 6],
    boxes: Vec<DashboardBox>,
}

fn clip_rect(rect: OverlayRect, extent: vk::Extent2D) -> Option<OverlayRect> {
    if rect.x >= extent.width || rect.y >= extent.height {
        return None;
    }
    let width = rect.width.min(extent.width - rect.x);
    let height = rect.height.min(extent.height - rect.y);
    if width == 0 || height == 0 {
        return None;
    }
    Some(OverlayRect {
        width,
        height,
        ..rect
    })
}

impl Dashboard {
    pub fn prepare(self, extent: vk::Extent2D) -> PreparedDashboard {
        let convert = |rectangles: Vec<OverlayRect>| {
            rectangles
                .into_iter()
                .filter_map(|rect| clip_rect(rect, extent))
                .map(|rect| {
                    vk::ClearRect::default()
                        .rect(
                            vk::Rect2D::default()
                                .offset(vk::Offset2D {
                                    x: rect.x as i32,
                                    y: rect.y as i32,
                                })
                                .extent(vk::Extent2D {
                                    width: rect.width,
                                    height: rect.height,
                                }),
                        )
                        .base_array_layer(0)
                        .layer_count(1)
                })
                .collect()
        };
        let groups = self.groups.map(convert);
        let edges = self.edges.map(|levels| {
            levels.map(|rectangles| {
                rectangles
                    .into_iter()
                    .filter_map(|rect| clip_rect(rect, extent))
                    .collect()
            })
        });
        PreparedDashboard {
            width: self.width,
            height: self.height,
            scale: self.scale,
            groups,
            edges,
            boxes: self.boxes,
        }
    }

    fn rect(&mut self, group: usize, x: u32, y: u32, width: u32, height: u32) {
        self.groups[group].push(OverlayRect {
            x: x * self.scale,
            y: y * self.scale,
            width: width * self.scale,
            height: height * self.scale,
        });
    }

    fn styled_box(&mut self, x: u32, y: u32, width: u32, height: u32, style: BoxStyle) {
        self.boxes.push(DashboardBox {
            rect: OverlayRect {
                x: x * self.scale,
                y: y * self.scale,
                width: width * self.scale,
                height: height * self.scale,
            },
            style,
        });
    }

    fn text(
        &mut self,
        text: &str,
        x: u32,
        y: u32,
        size: u32,
        width: u32,
        bold: bool,
        group: usize,
    ) -> u32 {
        let mut cache = FONTS
            .get_or_init(|| Mutex::new(FontCache::new()))
            .lock()
            .expect("font cache poisoned");
        let x = x * self.scale;
        let y = y * self.scale;
        let width = width * self.scale;
        let mut size = size * self.scale;
        let min_size = 8 * self.scale;
        let text_width = |cache: &mut FontCache, text: &str, size| {
            text.chars()
                .map(|c| cache.glyph(c, size, bold).metrics.advance_width)
                .sum::<f32>()
        };
        while size > min_size && text_width(&mut cache, text, size) > width as f32 {
            size -= self.scale;
        }
        let mut visible = text.to_owned();
        if text_width(&mut cache, &visible, size) > width as f32 {
            while !visible.is_empty()
                && text_width(&mut cache, &format!("{visible}…"), size) > width as f32
            {
                visible.pop();
            }
            visible.push('…');
        }
        let mut cursor = x as f32;
        let limit = x.saturating_add(width);
        let mut right = x;
        for character in visible.chars() {
            let glyph = cache.glyph(character, size, bold);
            let gx = cursor.round() as i32 + glyph.metrics.xmin;
            let gy = y as i32 + size as i32 - glyph.metrics.height as i32 - glyph.metrics.ymin;
            for &(dx, dy, span_width, coverage) in &glyph.spans {
                let left = gx + dx as i32;
                let top = gy + dy as i32;
                if left >= x as i32 && top >= 0 && left < limit as i32 {
                    let rect = OverlayRect {
                        x: left as u32,
                        y: top as u32,
                        width: span_width.min(limit - left as u32),
                        height: 1,
                    };
                    right = right.max(rect.x + rect.width);
                    if coverage == COVERAGE_LEVELS {
                        self.groups[group].push(rect);
                    } else {
                        self.edges[group][coverage - 1].push(rect);
                    }
                }
            }
            cursor += glyph.metrics.advance_width;
            if cursor >= limit as f32 {
                break;
            }
        }
        right.div_ceil(self.scale)
    }
}

impl PreparedDashboard {
    pub fn draw(
        &self,
        device: &ash::Device,
        command_buffer: vk::CommandBuffer,
        extent: vk::Extent2D,
        pipeline: vk::Pipeline,
        pipeline_layout: vk::PipelineLayout,
        opacity: f32,
        fps_color: [u8; 3],
    ) {
        if extent.width == 0 || extent.height == 0 {
            return;
        }
        let scale = self.scale;
        let panel_width = self.width * scale;
        let panel_height = self.height * scale;
        unsafe {
            device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::GRAPHICS, pipeline)
        };
        let panel = OverlayRect {
            x: 0,
            y: 0,
            width: panel_width,
            height: panel_height,
        };
        let shadow = OverlayRect {
            x: 2 * scale,
            y: 3 * scale,
            ..panel
        };
        if opacity > 0.0 {
            draw_rectangles(
                device,
                command_buffer,
                pipeline_layout,
                extent,
                &[shadow],
                [0.0, 0.0, 0.0, opacity * 0.3],
                10.0 * scale as f32,
                0.0,
            );
            draw_rectangles(
                device,
                command_buffer,
                pipeline_layout,
                extent,
                &[panel],
                [0.035, 0.045, 0.06, opacity.clamp(0.0, 1.0)],
                9.0 * scale as f32,
                -1.0,
            );
        }
        draw_rectangles(
            device,
            command_buffer,
            pipeline_layout,
            extent,
            &[panel],
            [0.50, 0.57, 0.65, 0.9],
            9.0 * scale as f32,
            1.2 * scale as f32,
        );
        for item in &self.boxes {
            let (fill, stroke, radius) = match item.style {
                BoxStyle::Mode => ([0.18, 0.13, 0.025, 0.75], [1.0, 0.75, 0.0, 1.0], 11.0),
                BoxStyle::Key => ([0.15, 0.18, 0.22, 0.9], [0.38, 0.45, 0.54, 1.0], 5.0),
                BoxStyle::ActiveKey => ([0.30, 0.20, 0.08, 0.9], [1.0, 0.72, 0.20, 1.0], 5.0),
            };
            draw_rectangles(
                device,
                command_buffer,
                pipeline_layout,
                extent,
                &[item.rect],
                fill,
                radius * scale as f32,
                -1.0,
            );
            draw_rectangles(
                device,
                command_buffer,
                pipeline_layout,
                extent,
                &[item.rect],
                stroke,
                radius * scale as f32,
                scale as f32,
            );
        }
        let [red, green, blue] = fps_color.map(|channel| f32::from(channel) / 255.0);
        let colors = [
            [0.55, 0.62, 0.70, 1.0],
            [0.92, 0.94, 0.97, 1.0],
            [red, green, blue, 1.0],
            [1.0, 0.72, 0.20, 1.0],
            [0.28, 0.33, 0.39, 1.0],
            [1.0, 0.75, 0.0, 1.0],
        ];
        for (group, (rectangles, color)) in self.groups.iter().zip(colors).enumerate() {
            if !rectangles.is_empty() {
                let attachment = [vk::ClearAttachment::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .color_attachment(0)
                    .clear_value(vk::ClearValue {
                        color: vk::ClearColorValue { float32: color },
                    })];
                unsafe { device.cmd_clear_attachments(command_buffer, &attachment, rectangles) };
            }
            for (index, edges) in self.edges[group].iter().enumerate() {
                let mut color = color;
                color[3] = (index + 1) as f32 / COVERAGE_LEVELS as f32;
                for chunk in edges.chunks(RECTANGLES_PER_DRAW) {
                    draw_rectangles(
                        device,
                        command_buffer,
                        pipeline_layout,
                        extent,
                        chunk,
                        color,
                        0.0,
                        0.0,
                    );
                }
            }
        }
    }
}

fn draw_rectangles(
    device: &ash::Device,
    command_buffer: vk::CommandBuffer,
    pipeline_layout: vk::PipelineLayout,
    extent: vk::Extent2D,
    rectangles: &[OverlayRect],
    color: [f32; 4],
    radius: f32,
    border: f32,
) {
    debug_assert!(!rectangles.is_empty() && rectangles.len() <= RECTANGLES_PER_DRAW);
    let mut constants = [0.0f32; 32];
    for (index, rect) in rectangles.iter().enumerate() {
        let x = rect.x as f32;
        let y = rect.y as f32;
        constants[index * 4..index * 4 + 4].copy_from_slice(&[
            2.0 * x / extent.width as f32 - 1.0,
            2.0 * y / extent.height as f32 - 1.0,
            2.0 * (x + rect.width as f32) / extent.width as f32 - 1.0,
            2.0 * (y + rect.height as f32) / extent.height as f32 - 1.0,
        ]);
    }
    constants[24..28].copy_from_slice(&color);
    constants[28..32].copy_from_slice(&[extent.width as f32, extent.height as f32, radius, border]);
    let bytes = unsafe {
        std::slice::from_raw_parts(
            constants.as_ptr().cast::<u8>(),
            std::mem::size_of_val(&constants),
        )
    };
    unsafe {
        device.cmd_push_constants(
            command_buffer,
            pipeline_layout,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            0,
            bytes,
        );
        device.cmd_draw(command_buffer, 6, rectangles.len() as u32, 0, 0);
    }
}

pub(super) fn scale_for(mode: OverlayMode, extent: vk::Extent2D, threshold: u32) -> u32 {
    let (width, height) = if mode == OverlayMode::Full {
        (WIDTH, HEIGHT)
    } else {
        (142, 62)
    };
    if extent.width >= threshold && width * 2 <= extent.width && height * 2 <= extent.height {
        2
    } else {
        1
    }
}

pub(super) fn layout(
    metrics: &MetricsSnapshot,
    mode: OverlayMode,
    target_fps: Option<f32>,
    auto_algorithm_index: Option<usize>,
    auto_algorithm_label: Option<&str>,
    hysteresis_percent: Option<f32>,
    auto_target: Option<(AutoMetric, AutoTarget)>,
    hotkeys: [Hotkey; 4],
    manual_step_fps: f32,
    feedback: OverlayFeedback,
    scale: u32,
) -> Dashboard {
    let full = mode == OverlayMode::Full;
    let mut dashboard = Dashboard {
        width: if full { WIDTH } else { 142 },
        height: if full { HEIGHT } else { 62 },
        scale,
        groups: std::array::from_fn(|_| Vec::new()),
        edges: std::array::from_fn(|_| std::array::from_fn(|_| Vec::new())),
        boxes: Vec::new(),
    };
    let fps = Renderer::fps_value(metrics);
    if !full {
        dashboard.text("FPS", 12, 8, 12, 110, false, LABEL);
        dashboard.text(
            &fps,
            12,
            23,
            if fps == "COLLECT" { 18 } else { 28 },
            120,
            true,
            ACCENT,
        );
        return dashboard;
    }
    dashboard.text("FRAMECAP", 16, 8, 12, 120, true, LABEL);
    let fps_right = dashboard.text(
        &fps,
        16,
        27,
        if fps == "COLLECT" { 20 } else { 36 },
        FPS_VALUE_WIDTH,
        true,
        ACCENT,
    );
    dashboard.text("FPS", fps_right + FPS_LABEL_GAP, 48, 14, 30, true, VALUE);
    let low = metrics.low_10_fps();
    let low = if low.is_finite() && low >= 0.0 {
        format!("{low:.0} FPS")
    } else {
        "COLLECT".to_owned()
    };
    let jitter = metrics.jitter_ms();
    let jitter = if jitter.is_finite() && jitter >= 0.0 {
        format!("{jitter:.1} ms")
    } else {
        "COLLECT".to_owned()
    };
    for (x, width, label, value) in [(150, 110, "10% LOW", low), (269, 130, "JITTER", jitter)] {
        dashboard.text(label, x, 29, 12, width, true, LABEL);
        dashboard.text(&value, x, 45, 18, width, true, VALUE);
    }
    dashboard.rect(LINE, 16, 78, WIDTH - 32, 1);
    let mode_label = match (target_fps, auto_algorithm_label) {
        (_, Some(label)) => format!("AUTO · {label}"),
        (Some(fps), None) if fps.is_finite() && fps > 0.0 => "MANUAL".to_owned(),
        _ => "OFF".to_owned(),
    };
    dashboard.styled_box(16, 86, 104, 23, BoxStyle::Mode);
    dashboard.text(&mode_label, 22, 89, 13, 92, true, MODE);
    let cap = target_fps
        .filter(|v| v.is_finite() && *v > 0.0)
        .map(format_fps)
        .unwrap_or_else(|| "OFF".into());
    let hys = hysteresis_percent
        .map(|v| format!("{v:.1}%"))
        .unwrap_or_else(|| "--".into());
    let target = auto_target
        .map(|v| format_auto_target(v, metrics))
        .unwrap_or_else(|| "--".into());
    dashboard.text(
        "CAP",
        128,
        87,
        11,
        45,
        true,
        if feedback.cap_changed {
            FEEDBACK
        } else {
            LABEL
        },
    );
    dashboard.text(
        &format!("{cap} FPS"),
        128,
        99,
        14,
        72,
        true,
        if feedback.cap_changed {
            FEEDBACK
        } else {
            VALUE
        },
    );
    dashboard.text(
        "HYS",
        202,
        87,
        11,
        45,
        true,
        if feedback.hysteresis_changed {
            FEEDBACK
        } else {
            LABEL
        },
    );
    dashboard.text(
        &hys,
        202,
        99,
        14,
        56,
        true,
        if feedback.hysteresis_changed {
            FEEDBACK
        } else {
            VALUE
        },
    );
    dashboard.text("TARGET", 259, 87, 11, 70, true, LABEL);
    dashboard.text(&target, 259, 99, 14, 140, true, VALUE);
    dashboard.rect(LINE, 16, 120, WIDTH - 32, 1);
    let hardware = metrics.hardware_metrics();
    let display = |value: Option<f32>, precision: usize, unit: &str| {
        value
            .filter(|v| v.is_finite())
            .map(|v| format!("{v:.precision$} {unit}"))
            .unwrap_or_else(|| "--".into())
    };
    let gpu_load = match metrics.gpu_measurement().status {
        TelemetryStatus::Available => display(Some(metrics.gpu_measurement().value), 0, "%"),
        TelemetryStatus::Expired => "STALE".into(),
        TelemetryStatus::Error => "ERR".into(),
        TelemetryStatus::Unsupported => "N/A".into(),
    };
    for (y, name, temp, vrm, load) in [
        (
            130,
            "GPU",
            hardware.gpu_edge_temp_c,
            hardware.gpu_vrm_temp_c,
            gpu_load,
        ),
        (
            157,
            "CPU",
            hardware.cpu_temp_c,
            hardware.cpu_vrm_temp_c,
            display(hardware.cpu_load_percent, 0, "%"),
        ),
    ] {
        for (x, value_width, label, value) in [
            (16, 68, name, display(temp, 0, "°C")),
            (138, 68, "VRM", display(vrm, 0, "°C")),
            (260, 85, "LOAD", load),
        ] {
            dashboard.text(label, x, y, 12, 50, true, LABEL);
            dashboard.text(&value, x + 54, y, 14, value_width, true, VALUE);
        }
    }
    dashboard.rect(LINE, 16, 183, WIDTH - 32, 1);
    dashboard.text("PPT", 16, 192, 12, 45, true, LABEL);
    dashboard.text(
        &display(hardware.ppt_w, 1, "W"),
        70,
        192,
        14,
        125,
        true,
        VALUE,
    );
    dashboard.text("FAN", 200, 192, 12, 45, true, LABEL);
    dashboard.text(
        &display(hardware.fan_rpm, 0, "RPM"),
        254,
        192,
        14,
        155,
        true,
        VALUE,
    );
    dashboard.rect(LINE, 16, 219, WIDTH - 32, 1);
    let (down, up) = if auto_algorithm_index.is_some() {
        ("-1%".into(), "+1%".into())
    } else if target_fps.is_some_and(|fps| fps.is_finite() && fps > 0.0) {
        (
            format!("-{} FPS", format_fps(manual_step_fps)),
            format!("+{} FPS", format_fps(manual_step_fps)),
        )
    } else {
        ("--".into(), "--".into())
    };
    for (x, width, hotkey, action) in [
        (16, 62, hotkeys[0], "MODE".to_owned()),
        (116, 62, hotkeys[2], down),
        (216, 62, hotkeys[1], up),
        (316, 47, hotkeys[3], "OSD".to_owned()),
    ] {
        let color = if feedback.hotkey == Some(hotkey) {
            FEEDBACK
        } else {
            LABEL
        };
        dashboard.styled_box(
            x,
            229,
            30,
            20,
            if feedback.hotkey == Some(hotkey) {
                BoxStyle::ActiveKey
            } else {
                BoxStyle::Key
            },
        );
        dashboard.text(hotkey.label(), x + 4, 231, 11, 23, true, color);
        dashboard.text(&action, x + 36, 231, 11, width, true, LABEL);
    }
    dashboard
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEYS: [Hotkey; 4] = [Hotkey::F9, Hotkey::F11, Hotkey::F10, Hotkey::F12];

    fn snapshot(fps: f32, hardware: HardwareMetrics, status: TelemetryStatus) -> MetricsSnapshot {
        let now = Instant::now();
        MetricsSnapshot::new_with_hardware_metrics(
            16.0,
            fps,
            if fps > 0.0 { 0.5 } else { -1.0 },
            if fps > 0.0 { 55.0 } else { -1.0 },
            GpuMeasurement {
                value: 55.0,
                timestamp: now,
                status,
                device_id: None,
            },
            hardware,
            now,
        )
    }

    fn assert_text(
        dashboard: &Dashboard,
        text: &str,
        x: u32,
        y: u32,
        size: u32,
        width: u32,
        group: usize,
    ) {
        let mut reference = Dashboard {
            width: WIDTH,
            height: HEIGHT,
            scale: dashboard.scale,
            groups: std::array::from_fn(|_| Vec::new()),
            edges: std::array::from_fn(|_| std::array::from_fn(|_| Vec::new())),
            boxes: Vec::new(),
        };
        reference.text(text, x, y, size, width, true, group);
        assert!(!reference.groups[group].is_empty());
        assert!(
            reference.groups[group]
                .iter()
                .all(|rect| dashboard.groups[group].contains(rect)),
            "missing {text:?}"
        );
        for (expected, actual) in reference.edges[group].iter().zip(&dashboard.edges[group]) {
            assert!(
                expected.iter().all(|rect| actual.contains(rect)),
                "missing antialiasing for {text:?}"
            );
        }
    }

    #[test]
    fn dashboard_displays_metrics_targets_and_feedback() {
        let metrics = snapshot(
            60.0,
            HardwareMetrics {
                gpu_edge_temp_c: Some(43.0),
                gpu_vrm_temp_c: Some(57.0),
                cpu_temp_c: Some(70.0),
                cpu_vrm_temp_c: Some(61.0),
                cpu_load_percent: Some(35.0),
                ppt_w: Some(32.6),
                fan_rpm: Some(1240.0),
                ..HardwareMetrics::default()
            },
            TelemetryStatus::Available,
        );
        let dashboard = layout(
            &metrics,
            OverlayMode::Full,
            Some(60.0),
            Some(0),
            Some("JIT"),
            Some(5.0),
            Some((AutoMetric::Fps, AutoTarget::Low10Fps)),
            KEYS,
            5.0,
            OverlayFeedback {
                hotkey: Some(Hotkey::F10),
                cap_changed: true,
                hysteresis_changed: true,
            },
            1,
        );
        assert_eq!((dashboard.width, dashboard.height), (415, 260));
        for (text, x, y, size, width, group) in [
            ("60", 16, 27, 36, FPS_VALUE_WIDTH, ACCENT),
            ("55 FPS", 150, 45, 18, 110, VALUE),
            ("0.5 ms", 269, 45, 18, 130, VALUE),
            ("AUTO · JIT", 22, 89, 13, 92, MODE),
            ("60 FPS", 128, 99, 14, 72, FEEDBACK),
            ("5.0%", 202, 99, 14, 56, FEEDBACK),
            ("55 FPS", 259, 99, 14, 140, VALUE),
            ("43 °C", 70, 130, 14, 68, VALUE),
            ("35 %", 314, 157, 14, 85, VALUE),
            ("32.6 W", 70, 192, 14, 125, VALUE),
            ("1240 RPM", 254, 192, 14, 155, VALUE),
            ("F10", 120, 231, 11, 23, FEEDBACK),
            ("-1%", 152, 231, 11, 62, LABEL),
        ] {
            assert_text(&dashboard, text, x, y, size, width, group);
        }
        assert!(dashboard
            .groups
            .iter()
            .flatten()
            .chain(dashboard.edges.iter().flatten().flatten())
            .all(|rect| rect.x + rect.width <= WIDTH && rect.y + rect.height <= HEIGHT));
        assert!(dashboard.boxes.iter().all(|item| {
            item.rect.x + item.rect.width <= WIDTH && item.rect.y + item.rect.height <= HEIGHT
        }));
        assert_eq!(
            dashboard
                .boxes
                .iter()
                .map(|item| item.style)
                .collect::<Vec<_>>(),
            [
                BoxStyle::Mode,
                BoxStyle::Key,
                BoxStyle::ActiveKey,
                BoxStyle::Key,
                BoxStyle::Key,
            ]
        );
    }

    #[test]
    fn dashboard_shows_collect_and_unavailable_states_and_fps_only() {
        let metrics = snapshot(-1.0, HardwareMetrics::default(), TelemetryStatus::Expired);
        let dashboard = layout(
            &metrics,
            OverlayMode::Full,
            None,
            None,
            None,
            None,
            None,
            KEYS,
            5.0,
            OverlayFeedback::default(),
            1,
        );
        assert_text(&dashboard, "COLLECT", 16, 27, 20, FPS_VALUE_WIDTH, ACCENT);
        assert_text(&dashboard, "COLLECT", 150, 45, 18, 110, VALUE);
        assert_text(&dashboard, "OFF", 22, 89, 13, 92, MODE);
        assert_text(&dashboard, "STALE", 314, 130, 14, 85, VALUE);
        assert_text(&dashboard, "--", 254, 192, 14, 155, VALUE);
        let fps_only = layout(
            &metrics,
            OverlayMode::Fps,
            None,
            None,
            None,
            None,
            None,
            KEYS,
            5.0,
            OverlayFeedback::default(),
            1,
        );
        assert_eq!((fps_only.width, fps_only.height), (142, 62));
        assert_text(&fps_only, "COLLECT", 12, 23, 18, 120, ACCENT);
        assert!(
            fps_only.groups[VALUE].is_empty()
                && fps_only.groups[FEEDBACK].is_empty()
                && fps_only.groups[MODE].is_empty()
        );
        assert!(fps_only.boxes.is_empty());
    }

    #[test]
    fn fps_label_tracks_value_without_colliding_with_next_column() {
        for scale in [1, 2] {
            for fps in [-1.0, 9.0, 60.0, 144.0, 9999.0, 999999.0] {
                let metrics = snapshot(fps, HardwareMetrics::default(), TelemetryStatus::Available);
                let dashboard = layout(
                    &metrics,
                    OverlayMode::Full,
                    None,
                    None,
                    None,
                    None,
                    None,
                    KEYS,
                    5.0,
                    OverlayFeedback::default(),
                    scale,
                );
                let right = dashboard.groups[ACCENT]
                    .iter()
                    .chain(dashboard.edges[ACCENT].iter().flatten())
                    .map(|rect| rect.x + rect.width)
                    .max()
                    .unwrap()
                    .div_ceil(scale);
                assert_text(&dashboard, "FPS", right + FPS_LABEL_GAP, 48, 14, 30, VALUE);
                assert!(
                    right + FPS_LABEL_GAP + 30 <= 150,
                    "fps={fps}, scale={scale}"
                );
            }
        }
    }

    #[test]
    fn common_values_keep_their_original_font_size() {
        for (text, size, width) in [
            ("9999", 36, FPS_VALUE_WIDTH),
            ("COLLECT", 20, FPS_VALUE_WIDTH),
            ("COLLECT", 18, 110),
            ("1000 FPS", 18, 110),
            ("240 FPS", 14, 72),
            ("100.0%", 14, 56),
            ("100 °C", 14, 68),
            ("-5 FPS", 11, 62),
            ("+5 FPS", 11, 62),
        ] {
            for scale in [1, 2] {
                let make_dashboard = || Dashboard {
                    width: WIDTH,
                    height: HEIGHT,
                    scale,
                    groups: std::array::from_fn(|_| Vec::new()),
                    edges: std::array::from_fn(|_| std::array::from_fn(|_| Vec::new())),
                    boxes: Vec::new(),
                };
                let mut constrained = make_dashboard();
                let mut unconstrained = make_dashboard();
                constrained.text(text, 16, 27, size, width, true, VALUE);
                unconstrained.text(text, 16, 27, size, 1000, true, VALUE);
                assert_eq!(
                    constrained.groups[VALUE], unconstrained.groups[VALUE],
                    "{text} at {scale}×"
                );
                assert_eq!(
                    constrained.edges[VALUE], unconstrained.edges[VALUE],
                    "{text} at {scale}×"
                );
            }
        }
    }

    #[test]
    fn prepared_dashboard_scales_only_when_it_fits_and_clips_to_swapchain() {
        let metrics = snapshot(60.0, HardwareMetrics::default(), TelemetryStatus::Available);
        for (extent, threshold, expected_scale) in [
            (
                vk::Extent2D {
                    width: 200,
                    height: 100,
                },
                100,
                1,
            ),
            (
                vk::Extent2D {
                    width: 1000,
                    height: 400,
                },
                100,
                1,
            ),
            (
                vk::Extent2D {
                    width: 1000,
                    height: 600,
                },
                100,
                2,
            ),
            (
                vk::Extent2D {
                    width: 1000,
                    height: 600,
                },
                2000,
                1,
            ),
            (
                vk::Extent2D {
                    width: WIDTH * 2 - 1,
                    height: HEIGHT * 2,
                },
                100,
                1,
            ),
            (
                vk::Extent2D {
                    width: WIDTH * 2,
                    height: HEIGHT * 2,
                },
                100,
                2,
            ),
        ] {
            let prepared = layout(
                &metrics,
                OverlayMode::Full,
                Some(60.0),
                None,
                None,
                None,
                None,
                KEYS,
                5.0,
                OverlayFeedback::default(),
                scale_for(OverlayMode::Full, extent, threshold),
            )
            .prepare(extent);
            assert_eq!(prepared.scale, expected_scale);
            assert!(prepared.groups.iter().flatten().all(|rect| {
                rect.rect.offset.x >= 0
                    && rect.rect.offset.y >= 0
                    && rect.rect.offset.x as u32 + rect.rect.extent.width <= extent.width
                    && rect.rect.offset.y as u32 + rect.rect.extent.height <= extent.height
            }));
            assert!(prepared.edges.iter().flatten().flatten().all(|rect| {
                rect.x + rect.width <= extent.width && rect.y + rect.height <= extent.height
            }));
            if expected_scale == 2 {
                assert!(prepared
                    .edges
                    .iter()
                    .flatten()
                    .flatten()
                    .all(|rect| rect.height == 1));
            }
        }
        assert_eq!(
            clip_rect(
                OverlayRect {
                    x: 8,
                    y: 9,
                    width: 4,
                    height: 3,
                },
                vk::Extent2D {
                    width: 10,
                    height: 10,
                }
            ),
            Some(OverlayRect {
                x: 8,
                y: 9,
                width: 2,
                height: 1,
            })
        );
        assert_eq!(
            clip_rect(
                OverlayRect {
                    x: 10,
                    y: 0,
                    width: 1,
                    height: 1,
                },
                vk::Extent2D {
                    width: 10,
                    height: 10,
                }
            ),
            None
        );
    }

    #[test]
    fn glyph_cache_preserves_quantized_coverage_at_both_scales() {
        let mut cache = FontCache::new();
        for size in [36, 72] {
            let (_, bitmap) = cache.bold.rasterize('6', size as f32);
            let mut expected = [0usize; COVERAGE_LEVELS + 1];
            for pixel in bitmap {
                let coverage = (usize::from(pixel) * COVERAGE_LEVELS + 127) / 255;
                if coverage > 0 {
                    expected[coverage] += 1;
                }
            }
            let glyph = cache.glyph('6', size, true);
            let mut counts = [0usize; COVERAGE_LEVELS + 1];
            for &(_, _, width, coverage) in &glyph.spans {
                counts[coverage] += width as usize;
            }
            assert_eq!(counts, expected);
            assert!(counts[1..COVERAGE_LEVELS].iter().sum::<usize>() > 30);
            assert!(counts[COVERAGE_LEVELS] > 30);
        }
    }
}
