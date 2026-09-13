use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::platform::windows::WindowAttributesExtWindows;
use winit::window::{Fullscreen, Window, WindowId};

use crate::capture;
use crate::config::{Config, DepthMode};
use crate::depth::{self, DepthGeometry};
use crate::power::{PowerEvent, PowerWatcher};
use crate::renderer::{Picture, Renderer, Uniforms};
use crate::simulation::Simulation;

pub fn run() -> Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new()?;
    event_loop.run_app(&mut app)?;
    Ok(())
}

const PAD_POINTS: u32 = 120;

// 合上盖子 → 立刻（等 LID_CLOSE_DELAY ms）播放。但系统会马上睡眠，
// 很多时候你看不到这一段，所以这个值保持固定，不做成配置。
const LID_CLOSE_DELAY: Duration = Duration::from_millis(150);

/// "操作提示"独立窗口里显示的内容。
const HELP_TEXT: &str = "\
【操作提示】
• 演示过程中按 Esc 提前结束
• 关闭窗口 = 最小化到托盘，双击托盘图标重新打开

【调参建议】
• 打开动画太慢 → 打开用时减到 600~800
• 觉得模糊太强 → 把最大模糊降到 80~100
• 想更亮一点 → 暗化调到 0.6 左右
• 想四角收敛更明显 → 后退调到 1.5~2.0

【参数含义】
[保持]
• 单位：毫秒，范围 0 ~ 3000，默认 500
• 含义：完全合上后，画面保持在'最模糊、最暗'的状态多久，然后才进入打开动画。
• 设为 0 → 关闭刚结束就立刻播放打开。

[起始角度]
• 单位：度，范围 5 ~ 130，默认 90
• 含义：低于这个角度，效果才开始出现。高于它，画面完全清晰。
• 例：默认 90°，意思是盖子从 130° 合到 90° 之间画质不变；到 90° 才开始出现模糊。
• 调大（比如 120）→ 刚开始合盖就有微效果。
• 调小（比如 60）→ 只有合得很深才看得到效果。

[满效果跨度]
• 单位：度，范围 5 ~ 60，默认 60
• 含义：从起始角度再关多少度，效果达到最强。
• 例：起始 90°、跨度 60°，那到 30° 时效果已满。之后角度再往下走，画面保持不变。
• 调大 → 效果更缓慢地累积。
• 调小 → 效果累积得更'陡'。
• 这条进度值同时喂给模糊和暗化两条曲线（下面两条）。
 
[最大模糊]
• 单位：点（像素），范围 10 ~ 200，默认 135
• 含义：效果满时，远端（远离铰链一侧）的模糊半径。
• 调大 → 远端糊得更厉害。
• 调小 → 模糊更'轻'。

[暗化]
• 范围 0 ~ 1，默认 1.0（即 100%）
• 含义：效果满时，远端叠加多少黑色。
• 0 → 只模糊不变暗；1 → 远端完全变黑。
• 着色器里对线性光做 pow(1 - max_dim * fade, 2.2)，这样调暗在视觉上接近'亮度百分比'。

[观察距离]
• 单位：屏幕高度的倍数，范围 1 ~ 6，默认 6.0
• 含义：眼睛到屏幕中心的距离。值越大，眼睛离屏幕越远，透视越'平'。
• 6 → 比较接近'坐着看笔记本'的正常透视。
• 1 → 眼睛贴到屏幕上，四角收敛得非常夸张，像俯冲。
• 和下一项'后退'共同决定画面四角的投影位置。

[后退]
• 范围 0 ~ 3，默认 1.0
• 含义：盖子每合上 1°，画面相对玻璃再向后转多少度。
• 1 → 画面跟着玻璃转，看起来像'贴在屏幕上'。
• 0 → 画面始终面向眼睛（不跟着玻璃转），你看到的是屏幕内容'浮在空中'。
• 3 → 画面比玻璃转得还快，四角向中心大幅塌缩，非常夸张。

[模糊均匀度]
• 范围 0 ~ 1，默认 0.0
• 含义：铰链侧（近端）的模糊占远端模糊的比例。
• 0 → 只有远端糊，铰链侧清晰，看起来'近清远糊'，有深度感。
• 1 → 整幅画面均匀糊，没有远近区分。
• 通常 0 ~ 0.3 比较自然。

[暗化延伸]
• 范围 0.2 ~ 1.0，默认 0.5
• 含义：从铰链侧往远端走，暗化在屏幕高度的哪个比例上达到满强度。
• 0.5 → 屏幕高度 50% 以上的部分完全变暗，下半部分渐变过渡。
• 设大（比如 1.0）→ 暗化铺满整个屏幕高度，过渡更柔和。
• 设小（比如 0.2）→ 只有最远端一点点变暗，其余基本不暗。
";

fn setup_fonts(ctx: &egui::Context) {
    let candidates = [
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\msyh.ttf",
        "C:\\Windows\\Fonts\\msyhbd.ttc",
        "C:\\Windows\\Fonts\\simhei.ttf",
        "C:\\Windows\\Fonts\\Deng.ttf",
        "C:\\Windows\\Fonts\\simsun.ttc",
    ];

    let mut fonts = egui::FontDefinitions::default();

    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            log::info!("loading CJK font: {path}");
            fonts.font_data.insert(
                "cjk".to_owned(),
                Arc::new(egui::FontData::from_owned(bytes)),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "cjk".to_owned());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("cjk".to_owned());
            ctx.set_fonts(fonts);
            return;
        }
    }

    log::warn!("no CJK font found, Chinese text will show as boxes");
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
enum DemoMode {
    Full,
    CloseOnly,
    OpenOnly,
}

struct App {
    config: Config,
    simulation: Simulation,
    geometry: DepthGeometry,
    renderer: Option<Renderer>,

    // ---- 主设置窗口 ----
    settings_window: Option<Arc<Window>>,
    settings_surface: Option<wgpu::Surface<'static>>,
    settings_config: Option<wgpu::SurfaceConfiguration>,
    egui_ctx: egui::Context,
    egui_state: Option<egui_winit::State>,
    egui_renderer: Option<egui_wgpu::Renderer>,

    // ---- 独立的操作提示窗口 ----
    help_window: Option<Arc<Window>>,
    help_surface: Option<wgpu::Surface<'static>>,
    help_config: Option<wgpu::SurfaceConfiguration>,
    help_egui_ctx: egui::Context,
    help_egui_state: Option<egui_winit::State>,
    help_egui_renderer: Option<egui_wgpu::Renderer>,

    // ---- 全屏演示窗口 ----
    demo_window: Option<Arc<Window>>,
    demo_surface: Option<wgpu::Surface<'static>>,
    demo_config: Option<wgpu::SurfaceConfiguration>,

    picture: Option<Picture>,
    pending_demo: Option<DemoMode>,
    pending_auto_demo: Option<(DemoMode, Instant)>,
    demo_active: bool,

    power_watcher: PowerWatcher,
}

impl App {
    fn new() -> Result<Self> {
        let config = Config::load();
        let sim = Simulation::new(&config);

        let egui_ctx = egui::Context::default();
        setup_fonts(&egui_ctx);

        let help_egui_ctx = egui::Context::default();
        setup_fonts(&help_egui_ctx);

        Ok(Self {
            config,
            simulation: sim,
            geometry: DepthGeometry::default(),
            renderer: None,
            settings_window: None,
            settings_surface: None,
            settings_config: None,
            egui_ctx,
            egui_state: None,
            egui_renderer: None,
            help_window: None,
            help_surface: None,
            help_config: None,
            help_egui_ctx,
            help_egui_state: None,
            help_egui_renderer: None,
            demo_window: None,
            demo_surface: None,
            demo_config: None,
            picture: None,
            pending_demo: None,
            pending_auto_demo: None,
            demo_active: false,
            power_watcher: PowerWatcher::start(),
        })
    }

    fn ensure_renderer(&mut self) -> Result<()> {
        if self.renderer.is_none() {
            self.renderer = Some(pollster::block_on(Renderer::new())?);
        }
        Ok(())
    }

    fn start_demo(&mut self, mode: DemoMode) -> Result<()> {
        let shot = capture::capture_primary_screen()?;

        self.ensure_renderer()?;
        let renderer = self.renderer.as_ref().unwrap();
        let picture = renderer.upload_picture(
            &shot.pixels,
            shot.width,
            shot.height,
            PAD_POINTS,
        )?;
        self.picture = Some(picture);

        if let Some(window) = self.demo_window.as_ref() {
            window.set_visible(true);
            window.focus_window();
            let _ = window.set_cursor_hittest(false);
            window.request_redraw();
        }
        self.demo_active = true;

        self.simulation.apply_config(&self.config);
        match mode {
            DemoMode::Full => self.simulation.play_full(),
            DemoMode::CloseOnly => self.simulation.play_close_only(),
            DemoMode::OpenOnly => self.simulation.play_open_only(),
        }
        Ok(())
    }

    fn stop_demo(&mut self) {
        self.simulation.stop();
        if let Some(window) = self.demo_window.as_ref() {
            let _ = window.set_cursor_hittest(true);
            window.set_visible(false);
        }
        self.picture = None;
        self.demo_active = false;
    }

    fn render_effect(&mut self, now: Instant) {
        let (Some(renderer), Some(picture), Some(surface), Some(cfg)) = (
            self.renderer.as_ref(),
            self.picture.as_ref(),
            self.demo_surface.as_ref(),
            self.demo_config.as_ref(),
        ) else {
            return;
        };

        let Some(angle) = self.simulation.current_angle(now) else {
            return;
        };

        let frame = match surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => return,
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let screen_w = cfg.width as f64;
        let screen_h = cfg.height as f64;

        let span = self.config.blur_span.max(1.0);
        let progress = ((self.config.threshold_angle - angle) / span).clamp(0.0, 1.0);
        let blur_strength = progress.powf(1.6);
        let dim_strength = progress.powf(0.7);

        let corners = self.geometry.corners(
            self.config.depth_mode,
            self.config.threshold_angle,
            angle,
            self.config.viewing_distance,
            self.config.recession,
            screen_w,
            screen_h,
        );
        let forward = depth::homography(screen_w, screen_h, &corners);
        let inv = depth::invert(&forward);

        let uniforms = Uniforms {
            column0: [inv[0][0] as f32, inv[0][1] as f32, inv[0][2] as f32, 0.0],
            column1: [inv[1][0] as f32, inv[1][1] as f32, inv[1][2] as f32, 0.0],
            column2: [inv[2][0] as f32, inv[2][1] as f32, inv[2][2] as f32, 0.0],
            screen_and_origin: [
                screen_w as f32,
                screen_h as f32,
                picture.padded_origin.0,
                picture.padded_origin.1,
            ],
            padded_and_blur: [
                picture.padded_size.0,
                picture.padded_size.1,
                self.config.max_blur_radius as f32,
                blur_strength as f32,
            ],
            shape: [
                self.config.blur_evenness as f32,
                self.config.max_dim as f32,
                1.0,
                picture.max_mip,
            ],
            light: [
                0.2,
                dim_strength as f32,
                self.config.dim_reach as f32,
                0.0,
            ],
        };

        renderer.render(&view, picture, &uniforms);
        frame.present();
    }

    fn render_settings(&mut self, now: Instant) {
        let (Some(window), Some(surface), Some(cfg)) = (
            self.settings_window.as_ref(),
            self.settings_surface.as_ref(),
            self.settings_config.as_ref(),
        ) else {
            return;
        };
        let (Some(egui_state), Some(egui_renderer)) =
            (self.egui_state.as_mut(), self.egui_renderer.as_mut())
        else {
            return;
        };
        let Some(renderer) = self.renderer.as_ref() else {
            return;
        };

        let raw_input = egui_state.take_egui_input(window);

        let config = &mut self.config;
        let angle_text = self
            .simulation
            .current_angle(now)
            .map(|a| format!("{:.1}°", a))
            .unwrap_or_else(|| "—".into());

        let mut show_help_requested = false;

        let ctx = self.egui_ctx.clone();
        let mut requested: Option<DemoMode> = None;

        let output = ctx.run(raw_input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("Windows Duo");
                ui.small("by kinhoy");
                ui.add_space(4.0);
                let _ = angle_text;
                ui.add_space(10.0);

                ui.group(|ui| {
                    ui.label("模拟开合时间");
                    ui.horizontal(|ui| {
                        ui.label("关闭用时");
                        ui.add(
                            egui::DragValue::new(&mut config.close_duration_ms)
                                .range(200..=6000)
                                .suffix(" ms"),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("保持");
                        ui.add(
                            egui::DragValue::new(&mut config.hold_duration_ms)
                                .range(0..=3000)
                                .suffix(" ms"),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("打开用时");
                        ui.add(
                            egui::DragValue::new(&mut config.open_duration_ms)
                                .range(200..=6000)
                                .suffix(" ms"),
                        );
                    });
                    ui.small("例：正常合盖用 2 秒，就把\"关闭用时\"设为 2000。");
                });

                ui.add_space(8.0);

                ui.group(|ui| {
                    ui.label("自动触发");
                    ui.checkbox(
                        &mut config.auto_demo_on_lid,
                        "合盖 / 开盖时自动播放对应动画",
                    );
                    ui.horizontal(|ui| {
                        ui.label("翻开后延迟");
                        ui.add(
                            egui::DragValue::new(&mut config.lid_open_delay_ms)
                                .range(100..=2000)
                                .suffix(" ms"),
                        );
                    });
                    ui.small(
                        "唤醒快（近几年的轻薄本）：300~500ms 就够；\n\
                         唤醒慢（老机型 / S3 睡眠）：1000~1500ms，太早抓屏会抓到黑屏。\n\
                         如果动画开始时有黑屏 → 调大；如果开始时画面已经完全清晰 → 调小。",
                    );
                });

                ui.add_space(8.0);

                ui.group(|ui| {
                    ui.label("形变模式");
                    ui.radio_value(
                        &mut config.depth_mode,
                        DepthMode::Perspective,
                        "透视投影（观察距离2.5~3.5最佳）",
                    )
                    .on_hover_text(
                        "把画面当作绕铰链向外旋转的平面，做真透视投影。\n\
                         每一行等比缩放，得到几何正确的等腰梯形，没有水平拉伸。\n\
                         看起来像一块真实的屏在翻转。",
                    );
                    ui.radio_value(
                        &mut config.depth_mode,
                        DepthMode::Stretch,
                        "梯形拉伸（观察距离6最佳）",
                    )
                    .on_hover_text(
                        "旧版：底边以中线为中心收缩成梯形，逐行水平压缩。\n\
                         形变更夸张、更有\"折叠\"感，但不是几何正确的透视。\n\
                         觉得新版\"翻\"得不够猛时，可以切回这个对比。",
                    );
                });

                ui.add_space(8.0);

                ui.group(|ui| {
                    ui.label("效果参数");
                    ui.add(egui::Slider::new(&mut config.threshold_angle, 5.0..=130.0).text("起始角度"));
                    ui.add(egui::Slider::new(&mut config.blur_span, 5.0..=60.0).text("满效果跨度"));
                    ui.add(egui::Slider::new(&mut config.max_blur_radius, 10.0..=200.0).text("最大模糊"));
                    ui.add(egui::Slider::new(&mut config.max_dim, 0.0..=1.0).text("暗化"));
                    ui.add(egui::Slider::new(&mut config.viewing_distance, 1.0..=6.0).text("观察距离"));
                    ui.add(egui::Slider::new(&mut config.recession, 0.0..=3.0).text("后退"));
                    ui.add(egui::Slider::new(&mut config.blur_evenness, 0.0..=1.0).text("模糊均匀度"));
                    ui.add(egui::Slider::new(&mut config.dim_reach, 0.2..=1.0).text("暗化延伸"));
                });

                ui.add_space(12.0);
                ui.label("手动演示");

                ui.horizontal(|ui| {
                    if ui
                        .add_sized([80.0, 28.0], egui::Button::new("合上盖子"))
                        .on_hover_text("只播放关闭动画")
                        .clicked()
                    {
                        requested = Some(DemoMode::CloseOnly);
                    }
                    if ui
                        .add_sized([80.0, 28.0], egui::Button::new("打开盖子"))
                        .on_hover_text("只播放打开动画")
                        .clicked()
                    {
                        requested = Some(DemoMode::OpenOnly);
                    }
                });

                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    if ui
                        .add_sized([80.0, 28.0], egui::Button::new("保存设置"))
                        .on_hover_text("写入 config.json")
                        .clicked()
                    {
                        config.save();
                    }
                    if ui
                        .add_sized([80.0, 28.0], egui::Button::new("重置参数"))
                        .on_hover_text("恢复所有参数为默认值（不会立即保存）")
                        .clicked()
                    {
                        *config = Config::default();
                    }
                });

                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    if ui
                        .add_sized([80.0, 28.0], egui::Button::new("操作提示"))
                        .on_hover_text("打开独立的操作提示窗口，可边看边调参")
                        .clicked()
                    {
                        show_help_requested = true;
                    }
                    if ui
                        .add_sized([60.0, 28.0], egui::Button::new("退出"))
                        .on_hover_text("真正退出程序（关闭窗口只会最小化到托盘）")
                        .clicked()
                    {
                        std::process::exit(0);
                    }
                });
            });
        });

        if let Some(mode) = requested {
            self.pending_demo = Some(mode);
        }

        // 直接访问 self.help_window 字段，避免借用整个 self 与 egui_renderer 冲突。
        if show_help_requested {
            if let Some(window) = self.help_window.as_ref() {
                window.set_visible(true);
                window.set_minimized(false);
                window.focus_window();
                window.request_redraw();
            }
        }

        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [cfg.width, cfg.height],
            pixels_per_point: output.pixels_per_point,
        };
        for (id, delta) in &output.textures_delta.set {
            egui_renderer.update_texture(&renderer.device, &renderer.queue, *id, delta);
        }

        let frame = match surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => return,
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = renderer.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("egui") },
        );
        egui_renderer.update_buffers(
            &renderer.device,
            &renderer.queue,
            &mut encoder,
            &primitives,
            &screen_descriptor,
        );
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            let mut pass = pass.forget_lifetime();
            egui_renderer.render(&mut pass, &primitives, &screen_descriptor);
        }
        renderer.queue.submit(Some(encoder.finish()));
        frame.present();

        for id in &output.textures_delta.free {
            egui_renderer.free_texture(id);
        }
    }

    /// 渲染独立的操作提示窗口。
    fn render_help(&mut self) {
        let (Some(window), Some(surface), Some(cfg)) = (
            self.help_window.as_ref(),
            self.help_surface.as_ref(),
            self.help_config.as_ref(),
        ) else {
            return;
        };
        let (Some(egui_state), Some(egui_renderer)) = (
            self.help_egui_state.as_mut(),
            self.help_egui_renderer.as_mut(),
        ) else {
            return;
        };
        let Some(renderer) = self.renderer.as_ref() else {
            return;
        };

        let raw_input = egui_state.take_egui_input(window);
        let ctx = self.help_egui_ctx.clone();

        let output = ctx.run(raw_input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add(
                        egui::Label::new(HELP_TEXT)
                            .wrap_mode(egui::TextWrapMode::Wrap),
                    );
                });
            });
        });

        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [cfg.width, cfg.height],
            pixels_per_point: output.pixels_per_point,
        };
        for (id, delta) in &output.textures_delta.set {
            egui_renderer.update_texture(&renderer.device, &renderer.queue, *id, delta);
        }

        let frame = match surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => return,
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = renderer.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("egui-help") },
        );
        egui_renderer.update_buffers(
            &renderer.device,
            &renderer.queue,
            &mut encoder,
            &primitives,
            &screen_descriptor,
        );
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui-help"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            let mut pass = pass.forget_lifetime();
            egui_renderer.render(&mut pass, &primitives, &screen_descriptor);
        }
        renderer.queue.submit(Some(encoder.finish()));
        frame.present();

        for id in &output.textures_delta.free {
            egui_renderer.free_texture(id);
        }
    }

    fn handle_power_events(&mut self) {
        while let Some(event) = self.power_watcher.try_recv() {
            match event {
                PowerEvent::TrayDoubleClick => {
                    if let Some(window) = self.settings_window.as_ref() {
                        window.set_visible(true);
                        window.set_minimized(false);
                        window.focus_window();
                    }
                }
                PowerEvent::LidOpened | PowerEvent::LidClosed => {
                    if !self.config.auto_demo_on_lid {
                        continue;
                    }
                    if self.simulation.is_running() || self.pending_auto_demo.is_some() {
                        continue;
                    }
                    let open_delay =
                        Duration::from_millis(self.config.lid_open_delay_ms.max(1));
                    let (mode, delay) = match event {
                        PowerEvent::LidOpened => (DemoMode::OpenOnly, open_delay),
                        PowerEvent::LidClosed => (DemoMode::CloseOnly, LID_CLOSE_DELAY),
                        PowerEvent::TrayDoubleClick => unreachable!(),
                    };
                    log::info!("auto demo scheduled: {mode:?} after {delay:?}");
                    self.pending_auto_demo = Some((mode, Instant::now() + delay));
                }
            }
        }

        if let Some((mode, when)) = self.pending_auto_demo {
            if Instant::now() >= when {
                self.pending_auto_demo = None;
                if !self.simulation.is_running() && self.pending_demo.is_none() {
                    self.pending_demo = Some(mode);
                }
            }
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.settings_window.is_some() {
            return;
        }

        self.ensure_renderer().unwrap();
        let renderer = self.renderer.as_ref().unwrap();

        // ---------- 主设置窗口 ----------
        let settings_window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Windows Duo")
                        .with_inner_size(winit::dpi::LogicalSize::new(360.0, 730.0))
                        .with_resizable(true),
                )
                .expect("failed to create settings window"),
        );

        let settings_surface: wgpu::Surface<'static> = renderer
            .instance
            .create_surface(settings_window.clone())
            .expect("failed to create settings surface");

        let settings_caps = settings_surface.get_capabilities(&renderer.adapter);
        let settings_format = settings_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(settings_caps.formats[0]);

        let settings_size = settings_window.inner_size();
        let settings_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: settings_format,
            width: settings_size.width.max(1),
            height: settings_size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: settings_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        settings_surface.configure(&renderer.device, &settings_config);

        let egui_state = egui_winit::State::new(
            self.egui_ctx.clone(),
            egui::ViewportId::ROOT,
            settings_window.as_ref(),
            Some(settings_window.scale_factor() as f32),
            None,
            None,
        );
        let egui_renderer = egui_wgpu::Renderer::new(
            &renderer.device,
            settings_config.format,
            None,
            1,
            false,
        );

        // ---------- 独立的操作提示窗口 ----------
        let help_window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("操作提示 - Windows Duo")
                        .with_inner_size(winit::dpi::LogicalSize::new(480.0, 560.0))
                        .with_min_inner_size(winit::dpi::LogicalSize::new(280.0, 200.0))
                        .with_resizable(true)
                        .with_visible(false),
                )
                .expect("failed to create help window"),
        );

        let help_surface: wgpu::Surface<'static> = renderer
            .instance
            .create_surface(help_window.clone())
            .expect("failed to create help surface");

        let help_caps = help_surface.get_capabilities(&renderer.adapter);
        let help_format = help_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(help_caps.formats[0]);

        let help_size = help_window.inner_size();
        let help_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: help_format,
            width: help_size.width.max(1),
            height: help_size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: help_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        help_surface.configure(&renderer.device, &help_config);

        let help_egui_state = egui_winit::State::new(
            self.help_egui_ctx.clone(),
            egui::ViewportId::ROOT,
            help_window.as_ref(),
            Some(help_window.scale_factor() as f32),
            None,
            None,
        );
        let help_egui_renderer = egui_wgpu::Renderer::new(
            &renderer.device,
            help_config.format,
            None,
            1,
            false,
        );

        // ---------- 全屏演示窗口 ----------
        let demo_window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Windows Duo Demo")
                        .with_fullscreen(Some(Fullscreen::Borderless(None)))
                        .with_visible(false)
                        .with_decorations(false)
                        .with_resizable(false)
                        .with_window_level(winit::window::WindowLevel::AlwaysOnTop)
                        .with_skip_taskbar(true),
                )
                .expect("failed to create demo window"),
        );

        let demo_surface: wgpu::Surface<'static> = renderer
            .instance
            .create_surface(demo_window.clone())
            .expect("failed to create demo surface");

        let demo_caps = demo_surface.get_capabilities(&renderer.adapter);
        let demo_format = demo_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(demo_caps.formats[0]);

        let demo_size = demo_window.inner_size();
        let demo_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: demo_format,
            width: demo_size.width.max(1),
            height: demo_size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: demo_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        demo_surface.configure(&renderer.device, &demo_config);

        self.settings_window = Some(settings_window);
        self.settings_surface = Some(settings_surface);
        self.settings_config = Some(settings_config);
        self.egui_state = Some(egui_state);
        self.egui_renderer = Some(egui_renderer);

        self.help_window = Some(help_window);
        self.help_surface = Some(help_surface);
        self.help_config = Some(help_config);
        self.help_egui_state = Some(help_egui_state);
        self.help_egui_renderer = Some(help_egui_renderer);

        self.demo_window = Some(demo_window);
        self.demo_surface = Some(demo_surface);
        self.demo_config = Some(demo_config);
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let is_settings = self
            .settings_window
            .as_ref()
            .map(|w| w.id() == window_id)
            .unwrap_or(false);
        let is_help = self
            .help_window
            .as_ref()
            .map(|w| w.id() == window_id)
            .unwrap_or(false);
        let is_demo = self
            .demo_window
            .as_ref()
            .map(|w| w.id() == window_id)
            .unwrap_or(false);

        if is_settings {
            if let (Some(state), Some(window)) =
                (self.egui_state.as_mut(), self.settings_window.as_ref())
            {
                let _ = state.on_window_event(window, &event);
            }
        } else if is_help {
            if let (Some(state), Some(window)) =
                (self.help_egui_state.as_mut(), self.help_window.as_ref())
            {
                let _ = state.on_window_event(window, &event);
            }
        }

        match event {
            WindowEvent::CloseRequested => {
                if is_settings {
                    if let Some(window) = self.settings_window.as_ref() {
                        window.set_visible(false);
                    }
                } else if is_help {
                    if let Some(window) = self.help_window.as_ref() {
                        window.set_visible(false);
                    }
                } else if is_demo {
                    if self.demo_active {
                        self.stop_demo();
                    }
                }
            }
            WindowEvent::Resized(size) if is_settings => {
                if let (Some(surface), Some(cfg), Some(renderer)) = (
                    self.settings_surface.as_ref(),
                    self.settings_config.as_mut(),
                    self.renderer.as_ref(),
                ) {
                    if size.width > 0 && size.height > 0 {
                        cfg.width = size.width;
                        cfg.height = size.height;
                        surface.configure(&renderer.device, cfg);
                    }
                }
            }
            WindowEvent::Resized(size) if is_help => {
                if let (Some(surface), Some(cfg), Some(renderer)) = (
                    self.help_surface.as_ref(),
                    self.help_config.as_mut(),
                    self.renderer.as_ref(),
                ) {
                    if size.width > 0 && size.height > 0 {
                        cfg.width = size.width;
                        cfg.height = size.height;
                        surface.configure(&renderer.device, cfg);
                    }
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(KeyCode::Escape),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => {
                if self.simulation.is_running() {
                    self.stop_demo();
                }
            }
            WindowEvent::RedrawRequested => {
                if is_settings {
                    self.render_settings(Instant::now());
                } else if is_help {
                    self.render_help();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        self.handle_power_events();

        if let Some(mode) = self.pending_demo.take() {
            if let Err(e) = self.start_demo(mode) {
                log::error!("start demo failed: {e:?}");
            }
        }

        if self.demo_active {
            let now = Instant::now();
            self.simulation.advance(now);
            if self.simulation.is_running() {
                self.render_effect(now);
            } else {
                self.stop_demo();
            }
            if let Some(window) = self.demo_window.as_ref() {
                window.request_redraw();
            }
        }

        if let Some(window) = self.settings_window.as_ref() {
            window.request_redraw();
        }

        // 只有帮助窗口处于可见状态时才持续重绘，避免后台空转。
        if let Some(window) = self.help_window.as_ref() {
            if window.is_visible().unwrap_or(false) {
                window.request_redraw();
            }
        }
    }
}