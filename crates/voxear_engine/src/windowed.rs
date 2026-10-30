use std::{sync::Arc, time::Instant};

use anyhow::{Context, Result};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use voxear_render::Renderer;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

use crate::engine::Engine;

/// Runs the engine in a window until it is closed, or for `frames` frames.
pub fn run_windowed(engine: Engine, title: &str, frames: Option<u64>) -> Result<()> {
    let event_loop = EventLoop::new().context("creating the event loop")?;
    let mut app = App { engine, title: title.to_string(), frames, renderer: None, window: None, last: None, error: None };
    event_loop.run_app(&mut app)?;
    match app.error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

struct App {
    engine: Engine,
    title: String,
    frames: Option<u64>,
    // Declared before the window so it is dropped first: the surface borrows the window.
    renderer: Option<Renderer>,
    window: Option<Arc<Window>>,
    last: Option<Instant>,
    error: Option<anyhow::Error>,
}

impl App {
    fn create_window(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let attributes = Window::default_attributes()
            .with_title(&self.title)
            .with_inner_size(LogicalSize::new(1280.0, 720.0));
        let window = Arc::new(event_loop.create_window(attributes)?);
        let size = window.inner_size();
        let display = window.display_handle()?.as_raw();
        let handle = window.window_handle()?.as_raw();
        let renderer = unsafe { Renderer::new(display, handle, size.width, size.height)? };
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
            renderer.render(self.engine.time().elapsed as f32)?;
        }
        if self.frames.is_some_and(|limit| self.engine.time().frame >= limit) {
            event_loop.exit();
        }
        Ok(())
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
