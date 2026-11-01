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
    keyboard::{Key, KeyCode, NamedKey, PhysicalKey},
    window::{CursorGrabMode, Fullscreen, Window, WindowId},
};

use voxear_ui::Key as UiKey;

use crate::{dev_console::ConsoleAction, fly_camera::FlyCamera, game::Game, stats_overlay::StatsOverlay};

/// Called once the renderer exists, typically to load the start level.
pub type Setup = Box<dyn FnOnce(&mut Game, &mut Renderer) -> Result<()>>;

/// How a game's window opens, and what it offers beside the game.
pub struct WindowOptions {
    pub title: String,
    /// Its size in logical pixels, when windowed.
    pub size: (u32, u32),
    /// Fill the screen, borderless, rather than open a window.
    pub fullscreen: bool,
    /// Stop after this many frames.
    pub frames: Option<u64>,
    /// Start with the stats overlay shown.
    pub stats: bool,
    /// The fly camera, the stats overlay on F1, the debug views and RenderDoc
    /// captures: for working on a game, not for playing it.
    pub tools: bool,
}

impl WindowOptions {
    /// A window for working on a game, with every tool, as `vox run` opens.
    pub fn tools(title: &str) -> Self {
        Self { title: title.to_string(), size: (1280, 720), fullscreen: false, frames: None, stats: false, tools: true }
    }
}

/// Runs the engine in a window until it is closed, or for `options.frames`
/// frames.
pub fn run_windowed(game: Game, options: WindowOptions, settings: RendererSettings, setup: Setup) -> Result<()> {
    let event_loop = event_loop(settings.renderdoc || RenderDoc::is_loaded()).context("creating the event loop")?;
    let mut app = App {
        game,
        settings,
        debug_view: DEBUG_VIEW_NONE,
        fly: FlyCamera::default(),
        overlay: StatsOverlay::new(options.stats),
        options,
        setup: Some(setup),
        renderer: None,
        window: None,
        last: None,
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

struct App {
    game: Game,
    settings: RendererSettings,
    debug_view: u32,
    fly: FlyCamera,
    options: WindowOptions,
    setup: Option<Setup>,
    // Declared before the window so it is dropped first: the surface borrows the window.
    renderer: Option<Renderer>,
    window: Option<Arc<Window>>,
    last: Option<Instant>,
    overlay: StatsOverlay,
    error: Option<anyhow::Error>,
}

impl App {
    fn create_window(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let (width, height) = self.options.size;
        let attributes = Window::default_attributes()
            .with_title(&self.options.title)
            .with_inner_size(LogicalSize::new(width as f64, height as f64))
            .with_fullscreen(self.options.fullscreen.then_some(Fullscreen::Borderless(None)));
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
        if self.options.tools {
            tracing::info!(
                "hold the right mouse button to fly: WASD to move, Q/E down/up, Shift faster, wheel for speed"
            );
            tracing::info!("F1 shows frame stats, and the key left of 1 opens the console");
        }
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
        if self.options.tools && self.overlay.update_console(Some(&mut self.game.engine)) == Some(ConsoleAction::Quit) {
            event_loop.exit();
        }
        if let (Some(renderer), Some(window)) = (self.renderer.as_mut(), &self.window) {
            self.overlay.update(delta, renderer);
            let size = window.inner_size();
            let scale = window.scale_factor() as f32;
            // The game's own UI over the whole window, under the tools.
            self.game.ui.draw(renderer, delta, [0, 0, size.width, size.height], scale)?;
            self.overlay.show_debug_texts(&self.game.engine.world, (size.width, size.height), scale);
            self.overlay.draw(renderer, (size.width, size.height), scale)?;
            renderer.render(&self.game.engine.world, self.game.engine.time().elapsed as f32)?;
        }
        if self.options.frames.is_some_and(|limit| self.game.engine.time().frame >= limit) {
            event_loop.exit();
        }
        Ok(())
    }

    /// Development keys until the editor exists: F1 shows frame stats; F12
    /// captures a frame in RenderDoc; F2 to F5 toggle the LOD, shadow
    /// cascade, light complexity and normals views.
    fn key(&mut self, key: &Key) {
        let Some(renderer) = self.renderer.as_mut().filter(|_| self.options.tools) else { return };
        let view = match key {
            Key::Named(NamedKey::F1) => return self.overlay.toggle(),
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

    /// The console's keys, before its text field sees them: the key left of 1
    /// opens and closes it, and while open, Up, Down, Tab and Escape are its.
    fn console_key(&mut self, event: &WindowEvent) -> bool {
        let WindowEvent::KeyboardInput { event, .. } = event else { return false };
        if event.state != ElementState::Pressed {
            return false;
        }
        if event.physical_key == PhysicalKey::Code(KeyCode::Backquote) {
            if !event.repeat {
                self.set_flying(false);
                self.overlay.toggle_console();
            }
            return true;
        }
        let key = match &event.logical_key {
            Key::Named(NamedKey::ArrowUp) => UiKey::Up,
            Key::Named(NamedKey::ArrowDown) => UiKey::Down,
            Key::Named(NamedKey::Tab) => UiKey::Tab,
            Key::Named(NamedKey::Escape) => UiKey::Escape,
            _ => return false,
        };
        self.overlay.console_key(key, Some(&self.game.engine))
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
        // The overlay sees input first; what it uses, the game does not. While
        // flying the cursor is captured and points at nothing, so the pointer is left out.
        let pointer = matches!(event, WindowEvent::CursorMoved { .. } | WindowEvent::MouseWheel { .. });
        if self.options.tools && self.console_key(&event) {
            return;
        }
        if self.options.tools && !(pointer && self.fly.is_looking()) && self.overlay.handle(&event) {
            return;
        }
        // Then the game's own UI, which takes what is over it.
        if !self.fly.is_looking() && !self.game.ui.is_empty() && crate::ui_input::handle(self.game.ui.ui(), &event) {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                if let PhysicalKey::Code(code) = event.physical_key
                    && self.options.tools
                {
                    self.fly.key(code, pressed);
                }
                if pressed && !event.repeat {
                    self.key(&event.logical_key);
                }
            }
            WindowEvent::MouseInput { state, button: MouseButton::Right, .. } if self.options.tools => {
                self.set_flying(state == ElementState::Pressed);
            }
            WindowEvent::MouseWheel { delta, .. } if self.options.tools => {
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
        if let DeviceEvent::MouseMotion { delta } = event
            && self.options.tools
        {
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
