use std::{sync::Arc, time::Instant};

use anyhow::{Context, Result};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use voxear_render::{Renderer, RendererSettings};
use voxear_rhi::WindowHandles;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, WindowEvent},
    keyboard::{Key, NamedKey},
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

use crate::engine::Engine;

/// Called once the renderer exists, to create the project's Objs and meshes.
pub type Setup = Box<dyn FnOnce(&mut Engine, &mut Renderer) -> Result<()>>;

/// How often the title bar's frame stats refresh.
const STATS_INTERVAL: f32 = 0.5;

/// Runs the engine in a window until it is closed, or for `frames` frames.
pub fn run_windowed(
    engine: Engine,
    title: &str,
    frames: Option<u64>,
    settings: RendererSettings,
    setup: Setup,
) -> Result<()> {
    let event_loop = EventLoop::new().context("creating the event loop")?;
    let mut app = App {
        engine,
        settings,
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

#[derive(Default)]
struct Stats {
    frames: u32,
    seconds: f32,
}

struct App {
    engine: Engine,
    settings: RendererSettings,
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
        let attributes = Window::default_attributes()
            .with_title(&self.title)
            .with_inner_size(LogicalSize::new(1280.0, 720.0));
        let window = Arc::new(event_loop.create_window(attributes)?);
        let size = window.inner_size();
        let handles = WindowHandles { display: window.display_handle()?.as_raw(), window: window.window_handle()?.as_raw() };
        let mut renderer = unsafe { Renderer::new(handles, size.width, size.height, &self.settings)? };
        if let Some(setup) = self.setup.take() {
            setup(&mut self.engine, &mut renderer)?;
        }
        self.renderer = Some(renderer);
        self.window = Some(window);
        Ok(())
    }

    fn frame(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let now = Instant::now();
        let delta = self.last.map_or(0.0, |last| (now - last).as_secs_f32());
        self.last = Some(now);
        self.engine.tick(delta);
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.render(&self.engine.world, self.engine.time().elapsed as f32)?;
        }
        self.update_stats(delta);
        if self.frames.is_some_and(|limit| self.engine.time().frame >= limit) {
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
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && !event.repeat
                    && event.logical_key == Key::Named(NamedKey::F12) =>
            {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.capture_next_frame();
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.frame(event_loop) {
                    self.fail(event_loop, error);
                }
            }
            _ => {}
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
