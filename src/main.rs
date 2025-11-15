#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use room_planner::{
    diagnostics::{self, Counters, Session},
    software::SoftwarePainter,
    ui::Workspace,
    updates,
};
use std::{
    num::NonZeroU32,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::{Icon, Window, WindowId},
};

#[derive(Debug)]
enum UserEvent {
    Wake,
    Repaint { deadline: Instant, pass: u64 },
    Access(egui_winit::accesskit_winit::Event),
}
impl From<egui_winit::accesskit_winit::Event> for UserEvent {
    fn from(event: egui_winit::accesskit_winit::Event) -> Self {
        Self::Access(event)
    }
}
struct WindowState {
    window: Arc<Window>,
    _software_context: softbuffer::Context<Arc<Window>>,
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    context: egui::Context,
    input: egui_winit::State,
    painter: SoftwarePainter,
    workspace: Workspace,
    deadline: Option<Instant>,
    occluded: bool,
    counters: Counters,
}
struct Application {
    state: Option<WindowState>,
    proxy: EventLoopProxy<UserEvent>,
    startup_marker: Option<PathBuf>,
    reopen: Option<PathBuf>,
    error: Option<String>,
    diagnostic: Option<Session>,
}
impl WindowState {
    fn visible(&self) -> bool {
        !self.occluded
            && self.window.is_visible() != Some(false)
            && self.window.is_minimized() != Some(true)
            && self.window.inner_size().width > 0
            && self.window.inner_size().height > 0
    }
    fn schedule(&mut self, deadline: Instant) {
        self.deadline = Some(self.deadline.map_or(deadline, |old| old.min(deadline)));
    }
    fn render(&mut self) -> Result<(), String> {
        if !self.visible() {
            return Ok(());
        }
        self.deadline = None;
        let mut input = self.input.take_egui_input(&self.window);
        egui_winit::update_viewport_info(
            input.viewports.entry(egui::ViewportId::ROOT).or_default(),
            &self.context,
            &self.window,
            false,
        );
        let output = self.context.run(input, |ctx| self.workspace.show(ctx));
        self.counters.layouts += 1;
        self.input
            .handle_platform_output(&self.window, output.platform_output);
        let delay = output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map(|o| o.repaint_delay)
            .unwrap_or(Duration::MAX);
        if let Some(deadline) = Instant::now().checked_add(delay) {
            self.schedule(deadline);
        }
        let primitives = self
            .context
            .tessellate(output.shapes, output.pixels_per_point);
        self.painter.update(&output.textures_delta)?;
        let size = self.window.inner_size();
        let width = NonZeroU32::new(size.width).ok_or("Zero width")?;
        let height = NonZeroU32::new(size.height).ok_or("Zero height")?;
        // Native softbuffer uses CPU DIB storage and GDI BitBlt on Windows. No GPU API is initialized.
        self.surface
            .resize(width, height)
            .map_err(|e| e.to_string())?;
        let mut buffer = self.surface.buffer_mut().map_err(|e| e.to_string())?;
        let start = Instant::now();
        self.painter.paint(
            &mut buffer,
            size.width as usize,
            size.height as usize,
            output.pixels_per_point,
            &primitives,
            egui::Color32::from_rgb(236, 241, 234),
        );
        self.counters.rasters += 1;
        self.counters.raster_micros += start.elapsed().as_micros() as u64;
        let start = Instant::now();
        self.window.pre_present_notify();
        buffer.present().map_err(|e| e.to_string())?;
        self.counters.presents += 1;
        self.counters.present_micros += start.elapsed().as_micros() as u64;
        self.painter.free(&output.textures_delta);
        Ok(())
    }
}
impl ApplicationHandler<UserEvent> for Application {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let result = (|| -> Result<WindowState, String> {
            let icon = Icon::from_rgba(
                include_bytes!(concat!(env!("OUT_DIR"), "/icon.rgba")).to_vec(),
                64,
                64,
            )
            .map_err(|e| e.to_string())?;
            let window = Arc::new(
                event_loop
                    .create_window(
                        Window::default_attributes()
                            // AccessKit must subclass the HWND before its first visible state.
                            .with_visible(false)
                            .with_title("RoomPlanner")
                            .with_inner_size(LogicalSize::new(1280.0, 820.0))
                            .with_min_inner_size(LogicalSize::new(1060.0, 680.0))
                            .with_window_icon(Some(icon)),
                    )
                    .map_err(|e| e.to_string())?,
            );
            let software_context =
                softbuffer::Context::new(window.clone()).map_err(|e| e.to_string())?;
            let surface = softbuffer::Surface::new(&software_context, window.clone())
                .map_err(|e| e.to_string())?;
            let context = egui::Context::default();
            let proxy = self.proxy.clone();
            context.set_request_repaint_callback(move |info| {
                if let Some(deadline) = Instant::now().checked_add(info.delay) {
                    let _ = proxy.send_event(UserEvent::Repaint {
                        deadline,
                        pass: info.current_cumulative_pass_nr,
                    });
                }
            });
            let mut input = egui_winit::State::new(
                context.clone(),
                egui::ViewportId::ROOT,
                window.as_ref(),
                Some(window.scale_factor() as f32),
                window.theme(),
                Some(8192),
            );
            input.init_accesskit(event_loop, &window, self.proxy.clone());
            let proxy = self.proxy.clone();
            let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
                let _ = proxy.send_event(UserEvent::Wake);
            });
            let mut workspace = if self.diagnostic.is_some() {
                let mut workspace = Workspace::with_settings(
                    room_planner::settings::Settings {
                        startup_updates: false,
                        reduced_motion: true,
                        ..Default::default()
                    },
                    Arc::new(updates::Https),
                    wake,
                );
                workspace
                    .editor
                    .replace(room_planner::model::Document::sample());
                workspace
            } else {
                Workspace::new(wake)
            };
            if let Some(path) = self.reopen.take()
                && let Err(error) = workspace.open_path(path)
            {
                workspace.report_open_error(error);
            }
            Ok(WindowState {
                window,
                _software_context: software_context,
                surface,
                context,
                input,
                painter: SoftwarePainter::default(),
                workspace,
                deadline: Some(Instant::now()),
                occluded: false,
                counters: Counters::default(),
            })
        })();
        match result {
            Ok(state) => {
                self.state = Some(state);
                if let Some(diagnostic) = &mut self.diagnostic {
                    diagnostic.started = Instant::now();
                }
                let state = self.state.as_ref().unwrap();
                state.window.set_visible(true);
                state.window.request_redraw();
            }
            Err(error) => {
                self.error = Some(error);
                event_loop.exit();
            }
        }
    }
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = &mut self.state else {
            return;
        };
        if state.window.id() != window_id {
            return;
        }
        let response = state.input.on_window_event(&state.window, &event);
        match event {
            WindowEvent::CloseRequested => {
                state.workspace.request_close();
                state.schedule(Instant::now());
            }
            WindowEvent::Occluded(occluded) => {
                state.occluded = occluded;
                if !occluded {
                    state.schedule(Instant::now());
                }
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                if state.visible() {
                    state.schedule(Instant::now());
                }
            }
            WindowEvent::RedrawRequested => {
                if state.visible() {
                    match state.render() {
                        Ok(()) => {
                            if let Some(path) = self.startup_marker.take() {
                                let _ = room_planner::storage::atomic_write(&path, b"started");
                            }
                        }
                        Err(error) => {
                            self.error = Some(error);
                            event_loop.exit();
                        }
                    }
                }
            }
            _ => {
                if response.repaint {
                    state.schedule(Instant::now());
                }
            }
        }
        if state.workspace.close_requested {
            event_loop.exit();
        }
    }
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        let Some(state) = &mut self.state else {
            return;
        };
        match event {
            UserEvent::Wake => {
                state.workspace.poll_jobs();
                state.schedule(Instant::now());
            }
            UserEvent::Repaint { deadline, pass } => {
                if state.context.cumulative_pass_nr() <= pass + 1 {
                    state.schedule(deadline);
                }
            }
            UserEvent::Access(event) => {
                if event.window_id != state.window.id() {
                    return;
                }
                match event.window_event {
                    egui_winit::accesskit_winit::WindowEvent::InitialTreeRequested => {
                        state.context.enable_accesskit()
                    }
                    egui_winit::accesskit_winit::WindowEvent::ActionRequested(request) => {
                        state.input.on_accesskit_action_request(request)
                    }
                    egui_winit::accesskit_winit::WindowEvent::AccessibilityDeactivated => {
                        state.context.disable_accesskit()
                    }
                }
                state.schedule(Instant::now());
            }
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(state) = &mut self.state else {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        };
        if let Some(diagnostic) = &mut self.diagnostic
            && diagnostic.due()
        {
            match diagnostic.phase {
                0 => diagnostic.snapshot("plan_idle_start", &state.counters),
                1 => {
                    diagnostic.snapshot("plan_idle_end", &state.counters);
                    state.window.set_visible(false);
                }
                2 => {
                    diagnostic.snapshot("hidden_end", &state.counters);
                    state.window.set_visible(true);
                    state.workspace.diagnostic_three_view();
                    state.schedule(Instant::now());
                }
                3 => diagnostic.snapshot("three_idle_start", &state.counters),
                4 => {
                    diagnostic.snapshot("three_idle_end", &state.counters);
                    if let Err(error) = diagnostic.save() {
                        self.error = Some(error);
                    }
                    event_loop.exit();
                }
                _ => {}
            }
            diagnostic.phase += 1;
        }
        let diagnostic_deadline = self.diagnostic.as_ref().and_then(Session::deadline);
        if !state.visible() {
            event_loop.set_control_flow(
                diagnostic_deadline.map_or(ControlFlow::Wait, ControlFlow::WaitUntil),
            );
            return;
        }
        match state.deadline {
            Some(deadline) if deadline <= Instant::now() => {
                state.deadline = None;
                state.window.request_redraw();
                event_loop.set_control_flow(ControlFlow::Wait);
            }
            Some(deadline) => event_loop.set_control_flow(ControlFlow::WaitUntil(
                diagnostic_deadline.map_or(deadline, |d| d.min(deadline)),
            )),
            None => event_loop.set_control_flow(
                diagnostic_deadline.map_or(ControlFlow::Wait, ControlFlow::WaitUntil),
            ),
        }
    }
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if std::env::var_os("ROOMPLANNER_RENDER_STATS").is_some()
            && let Some(state) = &self.state
        {
            eprintln!(
                "renderer=CPU/GDI layouts={} rasters={} presents={} raster_us={} present_us={}",
                state.counters.layouts,
                state.counters.rasters,
                state.counters.presents,
                state.counters.raster_micros,
                state.counters.present_micros
            );
        }
    }
}
fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|a| a == "--apply-update") {
        let path = args.get(1).ok_or("Missing update manifest")?;
        return updates::apply_update(&PathBuf::from(path));
    }
    let startup_marker = if args.first().is_some_and(|a| a == "--update-ready") {
        args.get(1).map(PathBuf::from)
    } else {
        None
    };
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .map_err(|e| e.to_string())?;
    let mut app = Application {
        state: None,
        proxy: event_loop.create_proxy(),
        startup_marker,
        reopen: args
            .iter()
            .position(|arg| arg == "--reopen")
            .and_then(|i| args.get(i + 1))
            .map(PathBuf::from),
        error: None,
        diagnostic: diagnostics::diagnostic_directory(&args)?
            .map(Session::new)
            .transpose()?,
    };
    event_loop.run_app(&mut app).map_err(|e| e.to_string())?;
    if let Some(error) = app.error {
        Err(error)
    } else {
        Ok(())
    }
}
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let directory = diagnostics::diagnostic_directory(&args).ok().flatten();
    diagnostics::initialize_error_reporting(
        directory
            .as_ref()
            .map(|p| p.join("startup-error.log"))
            .unwrap_or_else(diagnostics::default_error_path),
    );
    if let Err(error) = run() {
        let log = diagnostics::write_failure(&error).ok();
        eprintln!("RoomPlanner could not start: {error}");
        if directory.is_none() {
            let language = room_planner::settings::Settings::load()
                .unwrap_or_default()
                .language;
            let description = format!(
                "{}\n\n{error}{}",
                language.text("startup_failed"),
                log.map(|p| format!("\n\n{}: {}", language.text("error_log"), p.display()))
                    .unwrap_or_default()
            );
            rfd::MessageDialog::new()
                .set_title("RoomPlanner")
                .set_description(description)
                .set_level(rfd::MessageLevel::Error)
                .show();
        }
        std::process::exit(1);
    }
}
