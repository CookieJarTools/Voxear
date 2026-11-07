//! The editor's windows: input to the editor, and the right mouse button over
//! a Scene view flies its camera, as in Unity. On Wayland, files dragged
//! onto it are heard by `wayland_drops`, and passed on as winit passes them
//! elsewhere: the pointer moving, then files dropped.
//!
//! Panels floated out of the main window each get a desktop window of their
//! own, opened and closed as the editor asks (`Editor::wanted_windows`),
//! with an output of the renderer each, all drawn in the one frame. Events
//! go to the editor with the window they came from. While the left button
//! is held, the window it went down in keeps the pointer's moves wherever
//! it goes; on desktops that say where windows are, a move over another of
//! the editor's windows is handed to that one, from its own top left, so a
//! tab can be dragged from one window to another. Wayland says nothing of
//! where windows are, so there a tab dragged out of its window becomes the
//! desktop's own drag and drop, which tells each window it passes over (see
//! `wayland_drops`), and windows are moved by the desktop when their bar is
//! dragged.

use std::{collections::HashSet, sync::Arc, time::Instant};

use anyhow::{Context, Result};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use voxear_core::logging::LogCapture;
use voxear_engine::Project;
use voxear_render::{OutputId, Renderer, RendererSettings};
use voxear_rhi::WindowHandles;
use voxear_scripting::CursorLock;
use voxear_ui::{DesktopWindow, DockWindow};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize},
    event::{DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::PhysicalKey,
    window::{CursorGrabMode, CursorIcon, Window, WindowId},
};

use crate::editor::Editor;
#[cfg(target_os = "linux")]
use crate::wayland_drops::{Drag, WaylandDrops};

/// The least a desktop window of a panel shrinks to, in logical pixels.
const PANEL_MIN: (f64, f64) = (180.0, 120.0);

/// Opens the editor on `project` in a window, until it is closed.
pub fn run_editor(project: Project, settings: RendererSettings, log: LogCapture) -> Result<()> {
    let event_loop = EventLoop::new().context("creating the event loop")?;
    let mut app = App {
        project: Some(project),
        settings,
        log,
        editor: None,
        #[cfg(target_os = "linux")]
        drops: None,
        renderer: None,
        window: None,
        desktop: Vec::new(),
        pointer_in: None,
        pointer_at: None,
        held: None,
        carried: None,
        dropping: None,
        focused: HashSet::new(),
        drawn: false,
        scrub_cursor: false,
        game_cursor: None,
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
    /// Files dragged onto the window on Wayland. It borrows the window's
    /// connection, so it goes before the window does.
    #[cfg(target_os = "linux")]
    drops: Option<WaylandDrops>,
    // Declared before the windows so it is dropped first: the surfaces borrow them.
    renderer: Option<Renderer>,
    window: Option<Arc<Window>>,
    /// The desktop windows of panels floated out of the main window.
    desktop: Vec<PanelWindow>,
    /// The window the pointer is in, which a Scene view is flown from, and
    /// where in it the pointer last was.
    pointer_in: Option<WindowId>,
    pointer_at: Option<(WindowId, PhysicalPosition<f64>)>,
    /// The window the left button went down in, while it is held, and the
    /// one the pointer has been carried over to since, if another.
    held: Option<WindowId>,
    carried: Option<WindowId>,
    /// The window a drag of the desktop's (files, or a tab on Wayland) was
    /// last over.
    dropping: Option<Source>,
    /// The editor's windows that have the focus: one, or none while another
    /// program has it.
    focused: HashSet<WindowId>,
    /// A frame has been drawn since the windows last asked for one.
    drawn: bool,
    /// The windows show a left-right cursor, over a number's label that
    /// drags to change it.
    scrub_cursor: bool,
    /// How the mouse was last made to show and be held for the game, or
    /// `None` when it should be made again.
    game_cursor: Option<(bool, CursorLock)>,
    last: Option<Instant>,
    error: Option<anyhow::Error>,
}

/// A desktop window of a floating window of the dock's, and the output of
/// the renderer it shows.
struct PanelWindow {
    window: DesktopWindow,
    os: Arc<Window>,
    output: OutputId,
    title: String,
}

/// Which of the editor's windows an event came from.
#[derive(Clone, Copy, PartialEq)]
enum Source {
    Main,
    Panel(DesktopWindow),
}

impl Source {
    fn desktop(self) -> Option<DesktopWindow> {
        match self {
            Source::Main => None,
            Source::Panel(window) => Some(window),
        }
    }
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
        let mut editor = Editor::open(project, &mut renderer, self.log.clone())?;
        // Copying and pasting reach the desktop's clipboard, not only the editor's own.
        editor.ui.set_clipboard(Box::new(voxear_engine::ui_input::SystemClipboard::default()));
        window.set_ime_allowed(true);
        #[cfg(target_os = "linux")]
        {
            self.drops = WaylandDrops::new(window.display_handle()?.as_raw()).unwrap_or_else(|error| {
                tracing::warn!("files dragged onto the window will not be heard: {error:#}");
                None
            });
        }
        self.editor = Some(editor);
        self.renderer = Some(renderer);
        self.window = Some(window);
        Ok(())
    }

    fn frame(&mut self) -> Result<()> {
        let now = Instant::now();
        let delta = self.last.map_or(0.0, |last| (now - last).as_secs_f32());
        self.last = Some(now);
        let Some(window) = &self.window else { return Ok(()) };
        let size = window.inner_size();
        let scale = window.scale_factor();
        #[cfg(target_os = "linux")]
        self.pass_drags();
        let (Some(editor), Some(renderer)) = (&mut self.editor, &mut self.renderer) else { return Ok(()) };
        editor.update(renderer, (size.width, size.height), scale as f32, delta)?;
        let scrub = editor.ui.wants_scrub_cursor();
        if scrub != self.scrub_cursor {
            self.scrub_cursor = scrub;
            let icon = if scrub { CursorIcon::EwResize } else { CursorIcon::Default };
            for window in self.window.iter().chain(self.desktop.iter().map(|open| &open.os)) {
                window.set_cursor(icon);
            }
        }
        self.apply_game_cursor();
        Ok(())
    }

    /// Shows or hides the mouse, and holds it, in the window the game's
    /// pointer is in, as the game's scripts asked, while its view has the
    /// focus; the mouse is left alone while it flies the Scene camera.
    fn apply_game_cursor(&mut self) {
        let Some(editor) = &self.editor else { return };
        if editor.fly().is_looking() {
            self.game_cursor = None;
            return;
        }
        let wanted = editor.game_cursor().unwrap_or((true, CursorLock::None));
        if self.game_cursor == Some(wanted) {
            return;
        }
        let middle = editor.game_view_middle();
        let Some(from) = self.pointer_in.and_then(|id| self.from(id)) else { return };
        let Some(window) = self.os_window(from).cloned() else { return };
        let (visible, lock) = wanted;
        let grab = match lock {
            CursorLock::None => CursorGrabMode::None,
            CursorLock::Locked => CursorGrabMode::Locked,
            CursorLock::Confined => CursorGrabMode::Confined,
        };
        // Some systems lock only by confining, and some confine only by locking.
        if window.set_cursor_grab(grab).is_err() {
            let other = if grab == CursorGrabMode::Locked { CursorGrabMode::Confined } else { CursorGrabMode::Locked };
            let _ = window.set_cursor_grab(other);
        }
        window.set_cursor_visible(visible && lock != CursorLock::Locked);
        self.game_cursor = Some(wanted);
        // A locked mouse goes to the middle of the Game view, off the Play button it was over, and shows there
        // when let go.
        let Some((x, y)) = middle.filter(|_| lock == CursorLock::Locked) else { return };
        let position = PhysicalPosition::new(f64::from(x), f64::from(y));
        if window.set_cursor_position(position).is_ok()
            && let Some(editor) = self.editor.as_mut()
        {
            editor.handle_in(from.desktop(), &WindowEvent::CursorMoved { device_id: DeviceId::dummy(), position });
        }
    }

    /// Hands the editor what drags did on Wayland, as winit hands over a
    /// drop elsewhere: where the drag is, as the pointer in the window it is
    /// over, then each file, or the button let go for a panel's tab.
    #[cfg(target_os = "linux")]
    fn pass_drags(&mut self) {
        let Some(drags) = self.drops.as_mut().map(WaylandDrops::poll) else { return };
        let scale = self.window.as_ref().map_or(1.0, |window| window.scale_factor());
        let device_id = DeviceId::dummy();
        for drag in drags {
            let over = match drag {
                Drag::Over(surface, x, y) | Drag::PanelOver(surface, x, y) => Some((surface, x, y)),
                _ => None,
            };
            if let Some((surface, x, y)) = over {
                let Some(from) = self.with_surface(surface) else { continue };
                let position = PhysicalPosition::new(x * scale, y * scale);
                self.dropping = Some(from);
                if let Some(editor) = self.editor.as_mut() {
                    editor.handle_in(from.desktop(), &WindowEvent::CursorMoved { device_id, position });
                }
                continue;
            }
            let from = self.dropping.unwrap_or(Source::Main);
            let Some(editor) = self.editor.as_mut() else { return };
            let release =
                WindowEvent::MouseInput { device_id, state: ElementState::Released, button: MouseButton::Left };
            // Off every window, the tab is somewhere no window of the editor's is.
            let away = WindowEvent::CursorMoved { device_id, position: PhysicalPosition::new(-1e5, -1e5) };
            match drag {
                Drag::Dropped(paths) => {
                    for path in paths {
                        editor.handle_in(from.desktop(), &WindowEvent::DroppedFile(path));
                    }
                    self.dropping = None;
                }
                Drag::PanelAway => {
                    editor.handle_in(from.desktop(), &away);
                }
                Drag::PanelDropped => {
                    editor.handle_in(from.desktop(), &release);
                    (self.held, self.carried, self.dropping) = (None, None, None);
                }
                Drag::PanelCancelled => {
                    editor.handle_in(from.desktop(), &away);
                    editor.handle_in(from.desktop(), &release);
                    (self.held, self.carried, self.dropping) = (None, None, None);
                }
                Drag::Over(..) | Drag::PanelOver(..) => {}
            }
        }
    }

    /// The window whose surface a drag names.
    #[cfg(target_os = "linux")]
    fn with_surface(&self, surface: crate::wayland_drops::Surface) -> Option<Source> {
        let windows = self.window.iter().chain(self.desktop.iter().map(|open| &open.os));
        let window = windows.into_iter().find(|window| wayland_surface(window) == Some(surface))?;
        self.from(window.id())
    }

    /// On Wayland, a tab dragged out past its window's edge goes on as the
    /// desktop's drag and drop, which tells the window it goes over.
    #[cfg(target_os = "linux")]
    fn drag_tab_out(&mut self, id: WindowId, position: PhysicalPosition<f64>) {
        let (Some(drops), Some(editor)) = (self.drops.as_mut(), self.editor.as_ref()) else { return };
        if editor.ui.dragged_dock_tab().is_none() || self.dropping.is_some() {
            return;
        }
        let windows = self.window.iter().chain(self.desktop.iter().map(|open| &open.os));
        let Some(window) = windows.into_iter().find(|window| window.id() == id) else { return };
        let size = window.inner_size();
        let inside =
            (0.0..f64::from(size.width)).contains(&position.x) && (0.0..f64::from(size.height)).contains(&position.y);
        let surface = match window.window_handle().map(|handle| handle.as_raw()) {
            Ok(raw_window_handle::RawWindowHandle::Wayland(handle)) => handle.surface,
            _ => return,
        };
        if !inside && drops.drag_panel(surface) {
            self.dropping = self.from(id);
        }
    }

    /// Opens a desktop window for each floating window the editor wants on
    /// the desktop, and closes those it no longer does, the renderer's
    /// output first. Titles follow the tab showing.
    fn sync_windows(&mut self, event_loop: &ActiveEventLoop) {
        let (Some(editor), Some(renderer), Some(main)) = (&mut self.editor, &mut self.renderer, &self.window) else {
            return;
        };
        let wanted = editor.wanted_windows();
        let (kept, gone): (Vec<_>, Vec<_>) = std::mem::take(&mut self.desktop)
            .into_iter()
            .partition(|open| wanted.iter().any(|w| w.window == open.window));
        self.desktop = kept;
        for gone in gone {
            if let Some(output) = editor.forget_desktop_window(gone.window) {
                renderer.remove_output(output);
            }
            let id = gone.os.id();
            self.focused.remove(&id);
            for window in [&mut self.pointer_in, &mut self.held, &mut self.carried] {
                if *window == Some(id) {
                    *window = None;
                }
            }
        }
        let project = editor.game.project.name().to_string();
        let windows = || self.desktop.iter().map(|open| &open.os).chain(std::iter::once(main));
        let near = self.pointer_at.and_then(|(id, at)| place_near(windows().find(|w| w.id() == id)?, at));
        for want in &wanted {
            let title = format!("{} — {project}", want.title);
            if let Some(open) = self.desktop.iter_mut().find(|open| open.window == want.window) {
                if open.title != title {
                    open.os.set_title(&title);
                    open.title = title;
                }
                continue;
            }
            match open_panel_window(event_loop, main, renderer, want, &title, near) {
                Ok((os, output)) => {
                    let size = os.inner_size();
                    editor.open_desktop_window(want.window, output, (size.width, size.height));
                    self.desktop.push(PanelWindow { window: want.window, os, output, title });
                }
                Err(error) => {
                    tracing::warn!("could not open a window for {}: {error:#}", want.title);
                    editor.bring_back_desktop_window(want.window);
                }
            }
        }
    }

    fn from(&self, id: WindowId) -> Option<Source> {
        if self.window.as_ref().is_some_and(|window| window.id() == id) {
            return Some(Source::Main);
        }
        self.desktop.iter().find(|open| open.os.id() == id).map(|open| Source::Panel(open.window))
    }

    fn os_window(&self, from: Source) -> Option<&Arc<Window>> {
        match from {
            Source::Main => self.window.as_ref(),
            Source::Panel(window) => self.desktop.iter().find(|open| open.window == window).map(|open| &open.os),
        }
    }

    /// While the left button is held, a move over another of the editor's
    /// windows, on a desktop that says where windows are, goes to that one,
    /// from its own top left, and so does the release that follows.
    fn carry(&mut self, id: WindowId, from: Source, event: WindowEvent) -> (Source, WindowEvent) {
        match &event {
            WindowEvent::CursorMoved { device_id, position } if self.held == Some(id) => {
                let over = self.window_under(id, *position);
                self.carried = over.map(|(window, _)| window).filter(|&window| window != id);
                match over.filter(|&(window, _)| window != id) {
                    Some((window, local)) => {
                        let from = self.from(window).unwrap_or(from);
                        (from, WindowEvent::CursorMoved { device_id: *device_id, position: local })
                    }
                    None => (from, event),
                }
            }
            WindowEvent::MouseInput { state: ElementState::Released, button: MouseButton::Left, .. } => {
                let carried = self.carried.take().and_then(|window| self.from(window));
                self.held = None;
                (carried.unwrap_or(from), event)
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                (self.held, self.carried) = (Some(id), None);
                (from, event)
            }
            _ => (from, event),
        }
    }

    /// The editor's window the pointer is over, at `position` from the top
    /// left of window `id`, with where it is in that window, when the
    /// desktop says where windows are. Panels' windows come first, as they
    /// are usually over the main one.
    fn window_under(&self, id: WindowId, position: PhysicalPosition<f64>) -> Option<(WindowId, PhysicalPosition<f64>)> {
        let origin = self.from(id).and_then(|from| self.os_window(from))?.inner_position().ok()?;
        let (x, y) = (f64::from(origin.x) + position.x, f64::from(origin.y) + position.y);
        let windows = self.desktop.iter().map(|open| &open.os).chain(&self.window);
        windows.into_iter().find_map(|window| {
            let at = window.inner_position().ok()?;
            let size = window.inner_size();
            let local = PhysicalPosition::new(x - f64::from(at.x), y - f64::from(at.y));
            let inside =
                (0.0..f64::from(size.width)).contains(&local.x) && (0.0..f64::from(size.height)).contains(&local.y);
            inside.then_some((window.id(), local))
        })
    }

    /// Captures the pointer in `id` while flying a Scene camera, so it can
    /// turn without limit.
    fn set_flying(&mut self, flying: bool, id: Option<WindowId>) {
        let window = id.and_then(|id| self.from(id)).and_then(|from| self.os_window(from)).cloned();
        let Some(editor) = &mut self.editor else { return };
        if flying == editor.fly().is_looking() {
            return;
        }
        editor.fly_mut().set_looking(flying);
        let grab = if flying { CursorGrabMode::Locked } else { CursorGrabMode::None };
        let windows =
            window.into_iter().chain(self.window.clone()).chain(self.desktop.iter().map(|open| open.os.clone()));
        for window in windows {
            if window.set_cursor_grab(grab).is_err() && flying {
                let _ = window.set_cursor_grab(CursorGrabMode::Confined);
            }
            window.set_cursor_visible(!flying);
            if flying {
                // The window the button went down in alone holds the pointer.
                break;
            }
        }
    }

    /// Tells the editor the focus came or went when the editor as a whole
    /// gained or lost it, not as it moves between its own windows.
    fn focus(&mut self, id: WindowId, focused: bool) {
        let before = !self.focused.is_empty();
        if focused {
            self.focused.insert(id);
        } else {
            self.focused.remove(&id);
            self.set_flying(false, None);
        }
        let now = !self.focused.is_empty();
        if let (Some(editor), true) = (self.editor.as_mut(), before != now) {
            editor.handle(&WindowEvent::Focused(now));
        }
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

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(from) = self.from(id) else { return };
        let flying = self.editor.as_ref().is_some_and(|e| e.fly().is_looking());
        match &event {
            WindowEvent::CloseRequested => match from {
                Source::Main => return event_loop.exit(),
                // Its panels close, and the window with them once the editor no longer wants it.
                Source::Panel(window) => {
                    if let Some(editor) = self.editor.as_mut() {
                        editor.close_desktop_window(window);
                    }
                    return;
                }
            },
            WindowEvent::Resized(size) => match from {
                Source::Main => {
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.resize(size.width, size.height);
                    }
                }
                Source::Panel(window) => {
                    let output = self.desktop.iter().find(|open| open.window == window).map(|open| open.output);
                    if let (Some(renderer), Some(output)) = (self.renderer.as_mut(), output) {
                        let _ = renderer.resize_output(output, size.width, size.height);
                    }
                    if let Some(editor) = self.editor.as_mut() {
                        editor.resize_desktop_window(window, (size.width, size.height));
                    }
                    return;
                }
            },
            WindowEvent::Moved(at) => {
                if let (Source::Panel(window), Some(editor)) = (from, self.editor.as_mut()) {
                    editor.place_desktop_window(window, Some((at.x, at.y)));
                }
                return;
            }
            // Every window is drawn in the one frame, whichever asked first.
            WindowEvent::RedrawRequested => {
                if !std::mem::replace(&mut self.drawn, true)
                    && let Err(error) = self.frame()
                {
                    self.error = Some(error);
                    event_loop.exit();
                }
                return;
            }
            WindowEvent::Focused(focused) => return self.focus(id, *focused),
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer_at = Some((id, *position));
                if self.held.is_none() {
                    self.pointer_in = Some(id);
                }
            }
            WindowEvent::CursorEntered { .. } if self.held.is_none() => self.pointer_in = Some(id),
            _ => {}
        }
        let (from, event) = self.carry(id, from, event);
        let over_viewport = self.editor.as_ref().is_some_and(Editor::over_viewport);
        match &event {
            // The right button flies the Scene view from inside it; anywhere
            // else it is the UI's, for context menus.
            WindowEvent::MouseInput { state, button: MouseButton::Right, .. } if flying || over_viewport => {
                self.set_flying(*state == ElementState::Pressed, Some(id));
                return;
            }
            WindowEvent::KeyboardInput { event: key, .. } if flying => {
                if let (PhysicalKey::Code(code), Some(editor)) = (key.physical_key, self.editor.as_mut()) {
                    editor.fly_mut().key(code, key.state == ElementState::Pressed);
                }
                return;
            }
            WindowEvent::MouseWheel { delta, .. } if flying => {
                let notches = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(position) => position.y as f32 / 40.0,
                };
                if let Some(editor) = self.editor.as_mut() {
                    editor.fly_mut().scroll(notches);
                }
                return;
            }
            _ => {}
        }
        let Some(editor) = self.editor.as_mut() else { return };
        editor.handle_in(from.desktop(), &event);
        #[cfg(target_os = "linux")]
        if let WindowEvent::CursorMoved { position, .. } = event {
            self.drag_tab_out(id, position);
        }
        let Some(editor) = self.editor.as_mut() else { return };
        let pressed = matches!(event, WindowEvent::MouseInput { state: ElementState::Pressed, .. });
        // A desktop window's bar pressed: the desktop moves it, while the button is still down.
        if let Some(window) = editor.take_window_move().filter(|_| pressed)
            && let Some(os) = self.os_window(Source::Panel(window))
        {
            if let Err(error) = os.drag_window() {
                tracing::debug!("the desktop would not move the window: {error}");
            }
            (self.held, self.carried) = (None, None);
        }
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        if let (DeviceEvent::MouseMotion { delta }, Some(editor)) = (event, self.editor.as_mut()) {
            editor.fly_mut().look(delta.0, delta.1);
            editor.game_mouse_moved(delta.0, delta.1);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.sync_windows(event_loop);
        self.drawn = false;
        for window in self.window.iter().chain(self.desktop.iter().map(|open| &open.os)) {
            window.request_redraw();
        }
    }

    fn exiting(&mut self, _: &ActiveEventLoop) {
        #[cfg(target_os = "linux")]
        {
            self.drops = None;
        }
        self.editor = None;
        self.renderer = None;
        self.desktop.clear();
    }
}

/// Opens a desktop window for a floating window of the dock's, at its size
/// at the main window's scale, where it was when the desktop said, and an
/// output of the renderer for it.
fn open_panel_window(
    event_loop: &ActiveEventLoop,
    main: &Window,
    renderer: &mut Renderer,
    want: &DockWindow,
    title: &str,
    near: Option<PhysicalPosition<i32>>,
) -> Result<(Arc<Window>, OutputId)> {
    let scale = main.scale_factor();
    let size = PhysicalSize::new(
        (f64::from(want.size.0) * scale).round() as u32,
        (f64::from(want.size.1) * scale).round() as u32,
    );
    let least = PhysicalSize::new((PANEL_MIN.0 * scale) as u32, (PANEL_MIN.1 * scale) as u32);
    let mut attributes =
        Window::default_attributes().with_title(title).with_inner_size(size).with_min_inner_size(least);
    if let Some(at) = want.screen_at.map(|(x, y)| PhysicalPosition::new(x, y)).or(near) {
        attributes = attributes.with_position(at);
    }
    #[cfg(target_os = "linux")]
    {
        // Named apart from the main window, for desktops' rules about windows such as which float.
        use winit::platform::wayland::WindowAttributesExtWayland;
        attributes = WindowAttributesExtWayland::with_name(attributes, "voxear-editor-panel", "voxear-editor-panel");
    }
    let os = Arc::new(event_loop.create_window(attributes)?);
    let size = os.inner_size();
    let handles = WindowHandles { display: os.display_handle()?.as_raw(), window: os.window_handle()?.as_raw() };
    // Safety: the output is removed before the window closes (`sync_windows`, and the renderer drops first).
    let output = unsafe { renderer.add_window_output(handles, size.width, size.height)? };
    os.set_ime_allowed(true);
    Ok((os, output))
}

/// Where a window torn off at the pointer goes, on a desktop that says
/// where windows are: with its tab under the pointer, last seen at `at` in
/// `window`, maybe past its edge.
fn place_near(window: &Window, at: PhysicalPosition<f64>) -> Option<PhysicalPosition<i32>> {
    let origin = window.inner_position().ok()?;
    let scale = window.scale_factor();
    let (x, y) = (f64::from(origin.x) + at.x - 40.0 * scale, f64::from(origin.y) + at.y - 12.0 * scale);
    Some(PhysicalPosition::new(x as i32, y as i32))
}

/// A window's Wayland surface, to tell which window a drag is over.
#[cfg(target_os = "linux")]
fn wayland_surface(window: &Window) -> Option<crate::wayland_drops::Surface> {
    match window.window_handle().ok()?.as_raw() {
        raw_window_handle::RawWindowHandle::Wayland(handle) => Some(WaylandDrops::surface(handle.surface)),
        _ => None,
    }
}
