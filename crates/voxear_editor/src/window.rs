//! The editor's window: input to the editor, and the right mouse button over
//! the Scene view flies its camera, as in Unity.

use std::{sync::Arc, time::Instant};

use anyhow::{Context, Result};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use voxear_core::logging::LogCapture;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_rhi::WindowHandles;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::PhysicalKey,
    window::{CursorGrabMode, Window, WindowId},
};

use crate::editor::Editor;

/// Opens the editor on `project` in a window, until it is closed.
pub fn run_editor(project: Project, settings: RendererSettings, log: LogCapture) -> Result<()> {
    let event_loop = EventLoop::new().context("creating the event loop")?;
    let mut app = App {
        project: Some(project),
        settings,
        log,
        editor: None,
        renderer: None,
        window: None,
        last: None,
        error: None,
    };
    event_loop.run_app(&mut app)?;
    app.error.map_or(Ok(()), Err)
}

struct App {
    project: Option<Project>,
    settings: RendererSettings,
    log: LogCapture,
    editor: Option<Editor>,
    // Declared before the window so it is dropped first: the surface borrows the window.
    renderer: Option<Renderer>,
    window: Option<Arc<Window>>,
    last: Option<Instant>,
    error: Option<anyhow::Error>,
}

impl App {
    fn create(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let project = self.project.take().context("the editor opens once")?;
        let title = format!("{} — Voxear Editor", project.name());
        let attributes =
            Window::default_attributes().with_title(title).with_inner_size(LogicalSize::new(1440.0, 900.0));
        let window = Arc::new(event_loop.create_window(attributes)?);
        let size = window.inner_size();
        let handles =
            WindowHandles { display: window.display_handle()?.as_raw(), window: window.window_handle()?.as_raw() };
        let mut renderer = unsafe { Renderer::new(handles, size.width, size.height, &self.settings)? };
        let editor = Editor::open(project, &mut renderer, self.log.clone())?;
        window.set_ime_allowed(true);
        self.editor = Some(editor);
        self.renderer = Some(renderer);
        self.window = Some(window);
        Ok(())
    }

    fn frame(&mut self) -> Result<()> {
        let now = Instant::now();
        let delta = self.last.map_or(0.0, |last| (now - last).as_secs_f32());
        self.last = Some(now);
        let (Some(editor), Some(renderer), Some(window)) = (&mut self.editor, &mut self.renderer, &self.window) else {
            return Ok(());
        };
        let size = window.inner_size();
        editor.update(renderer, (size.width, size.height), window.scale_factor() as f32, delta)
    }

    /// Captures the pointer while flying the Scene camera, so it can turn without limit.
    fn set_flying(&mut self, flying: bool) {
        let (Some(editor), Some(window)) = (&mut self.editor, &self.window) else { return };
        if flying == editor.fly.is_looking() {
            return;
        }
        editor.fly.set_looking(flying);
        let grab = if flying { CursorGrabMode::Locked } else { CursorGrabMode::None };
        if window.set_cursor_grab(grab).is_err() && flying {
            let _ = window.set_cursor_grab(CursorGrabMode::Confined);
        }
        window.set_cursor_visible(!flying);
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none()
            && let Err(error) = self.create(event_loop)
        {
            self.error = Some(error);
            event_loop.exit();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let flying = self.editor.as_ref().is_some_and(|e| e.fly.is_looking());
        match &event {
            WindowEvent::CloseRequested => return event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.frame() {
                    self.error = Some(error);
                    event_loop.exit();
                }
                return;
            }
            WindowEvent::MouseInput { state, button: MouseButton::Right, .. } => {
                let over = self.editor.as_ref().is_some_and(Editor::over_viewport);
                let pressed = *state == ElementState::Pressed;
                if !pressed || over {
                    self.set_flying(pressed);
                }
                return;
            }
            WindowEvent::KeyboardInput { event: key, .. } if flying => {
                if let (PhysicalKey::Code(code), Some(editor)) = (key.physical_key, self.editor.as_mut()) {
                    editor.fly.key(code, key.state == ElementState::Pressed);
                }
                return;
            }
            WindowEvent::MouseWheel { delta, .. } if flying => {
                let notches = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(position) => position.y as f32 / 40.0,
                };
                if let Some(editor) = self.editor.as_mut() {
                    editor.fly.scroll(notches);
                }
                return;
            }
            WindowEvent::Focused(false) => self.set_flying(false),
            _ => {}
        }
        if let Some(editor) = self.editor.as_mut() {
            editor.handle(&event);
        }
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        if let (DeviceEvent::MouseMotion { delta }, Some(editor)) = (event, self.editor.as_mut()) {
            editor.fly.look(delta.0, delta.1);
        }
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn exiting(&mut self, _: &ActiveEventLoop) {
        self.editor = None;
        self.renderer = None;
    }
}
