use std::{sync::Arc, time::Instant};

use anyhow::{Context, Result};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use voxear_render::{
    DEBUG_VIEW_CASCADES, DEBUG_VIEW_LIGHT_COMPLEXITY, DEBUG_VIEW_LOD, DEBUG_VIEW_NONE, DEBUG_VIEW_NORMALS, Renderer,
    RendererSettings, ViewOptions,
};
use voxear_rhi::{RenderDoc, WindowHandles};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, NamedKey, PhysicalKey},
    window::{CursorGrabMode, Window, WindowId},
};

use crate::{fly_camera::FlyCamera, game::Game};

/// Called once the renderer exists, typically to load the start level.
pub type Setup = Box<dyn FnOnce(&mut Game, &mut Renderer) -> Result<()>>;

/// How often the title bar's frame stats refresh.
const STATS_INTERVAL: f32 = 0.5;

/// Runs the engine in a window until it is closed, or for `frames` frames.
pub fn run_windowed(
    game: Game,
    title: &str,
    frames: Option<u64>,
    settings: RendererSettings,
    setup: Setup,
) -> Result<()> {
    let event_loop = event_loop(settings.renderdoc || RenderDoc::is_loaded()).context("creating the event loop")?;
    let mut app = App {
        game,
        settings,
        debug_view: DEBUG_VIEW_NONE,
        fly: FlyCamera::default(),
        title: title.to_string(),
        frames,
        setup: Some(setup),
        renderer: None,
        window: None,
        last: None,
        stats: Stats::default(),
        error: None,
    };
    event_loop.run_app(&mut app)?;
    match app.error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

/// RenderDoc cannot capture Wayland windows (its layer hides
/// `VK_KHR_wayland_surface`), so with RenderDoc in the process the window
/// opens through X11 instead, which XWayland provides on Wayland desktops.
fn event_loop(renderdoc: bool) -> Result<EventLoop<()>, winit::error::EventLoopError> {
    #[cfg(all(unix, not(target_os = "macos")))]
    if renderdoc && std::env::var_os("WAYLAND_DISPLAY").is_some() {
        use winit::platform::x11::EventLoopBuilderExtX11;
        tracing::info!("RenderDoc cannot capture Wayland windows, so this run uses X11 (XWayland)");
        return EventLoop::builder().with_x11().build();
    }
    let _ = renderdoc;
    EventLoop::new()
}

#[derive(Default)]
struct Stats {
    frames: u32,
    seconds: f32,
}

struct App {
    game: Game,
    settings: RendererSettings,
    debug_view: u32,
    fly: FlyCamera,
    title: String,
    frames: Option<u64>,
    setup: Option<Setup>,
    // Declared before the window so it is dropped first: the surface borrows the window.
    renderer: Option<Renderer>,
    window: Option<Arc<Window>>,
    last: Option<Instant>,
    stats: Stats,
    error: Option<anyhow::Error>,
}

impl App {
    fn create_window(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let attributes =
            Window::default_attributes().with_title(&self.title).with_inner_size(LogicalSize::new(1280.0, 720.0));
        let window = Arc::new(event_loop.create_window(attributes)?);
        let size = window.inner_size();
        let handles =
            WindowHandles { display: window.display_handle()?.as_raw(), window: window.window_handle()?.as_raw() };
        let mut renderer = unsafe { Renderer::new(handles, size.width, size.height, &self.settings)? };
        if let Some(setup) = self.setup.take() {
            setup(&mut self.game, &mut renderer)?;
        }
        self.renderer = Some(renderer);
        self.window = Some(window);
        tracing::info!("hold the right mouse button to fly: WASD to move, Q/E down/up, Shift faster, wheel for speed");
        Ok(())
    }

    /// Captures the mouse while flying so it can turn without limit.
    fn set_flying(&mut self, flying: bool) {
        if flying == self.fly.is_looking() {
            return;
        }
        self.fly.set_looking(flying);
        let Some(window) = &self.window else { return };
        let grab = if flying { CursorGrabMode::Locked } else { CursorGrabMode::None };
        if window.set_cursor_grab(grab).is_err() && flying {
            let _ = window.set_cursor_grab(CursorGrabMode::Confined);
        }
        window.set_cursor_visible(!flying);
    }

    fn frame(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let now = Instant::now();
        let delta = self.last.map_or(0.0, |last| (now - last).as_secs_f32());
        self.last = Some(now);
        self.fly.update(&mut self.game.engine.world, delta);
        self.game.tick(delta, self.renderer.as_mut());
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.render(&self.game.engine.world, self.game.engine.time().elapsed as f32)?;
        }
        self.update_stats(delta);
        if self.frames.is_some_and(|limit| self.game.engine.time().frame >= limit) {
            event_loop.exit();
        }
        Ok(())
    }

    /// Shows frame rate and GPU time in the title until the editor has a stats overlay.
    fn update_stats(&mut self, delta: f32) {
        self.stats.frames += 1;
        self.stats.seconds += delta;
        if self.stats.seconds < STATS_INTERVAL {
            return;
        }
        let (Some(window), Some(renderer)) = (&self.window, &self.renderer) else { return };
        let fps = self.stats.frames as f32 / self.stats.seconds;
        let gpu_ms: f32 = renderer.timings().iter().map(|(_, ms)| ms).sum();
        window.set_title(&format!("{} · {fps:.0} fps · GPU {gpu_ms:.2} ms", self.title));
        self.stats = Stats::default();
    }

    /// Development keys until the editor exists: F12 captures a frame in
    /// RenderDoc; F2 to F5 toggle the LOD, shadow cascade, light complexity
    /// and normals views.
    fn key(&mut self, key: &Key) {
        let Some(renderer) = self.renderer.as_mut() else { return };
        let view = match key {
            Key::Named(NamedKey::F12) => return renderer.capture_next_frame(),
            Key::Named(NamedKey::F2) => DEBUG_VIEW_LOD,
            Key::Named(NamedKey::F3) => DEBUG_VIEW_CASCADES,
            Key::Named(NamedKey::F4) => DEBUG_VIEW_LIGHT_COMPLEXITY,
            Key::Named(NamedKey::F5) => DEBUG_VIEW_NORMALS,
            _ => return,
        };
        self.debug_view = if self.debug_view == view { DEBUG_VIEW_NONE } else { view };
        renderer.set_view_options(ViewOptions { debug_view: self.debug_view, ..ViewOptions::default() });
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: anyhow::Error) {
        self.error = Some(error);
        event_loop.exit();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none()
            && let Err(error) = self.create_window(event_loop)
        {
            self.fail(event_loop, error);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                if let PhysicalKey::Code(code) = event.physical_key {
                    self.fly.key(code, pressed);
                }
                if pressed && !event.repeat {
                    self.key(&event.logical_key);
                }
            }
            WindowEvent::MouseInput { state, button: MouseButton::Right, .. } => {
                self.set_flying(state == ElementState::Pressed);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let notches = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(position) => position.y as f32 / 40.0,
                };
                self.fly.scroll(notches);
            }
            WindowEvent::Focused(false) => self.set_flying(false),
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.frame(event_loop) {
                    self.fail(event_loop, error);
                }
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event {
            self.fly.look(delta.0, delta.1);
        }
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn exiting(&mut self, _: &ActiveEventLoop) {
        self.renderer = None;
    }
}
