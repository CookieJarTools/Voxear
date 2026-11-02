//! Panels floated out of the editor's window, each floating window in a
//! desktop window of its own (see `voxear_ui::DockWindow`). The dock says
//! which it wants; whoever owns the windows, the editor's `window` module or
//! a test, opens a window for each with an output of the renderer to draw
//! into, and hands it over with `open_desktop_window`. Each frame the editor
//! gives each its UI and the 3D views in it, and events from it come in
//! through `handle_in`. Closing one closes its panels, as Close on their
//! menus does, and a closed one opens again from the Window button.

use voxear_render::{OutputId, Renderer};
use voxear_ui::{DesktopWindow, DockWindow, NodeId};

use super::Editor;

/// A desktop window opened for one of the dock's floating windows: the
/// output it draws into and its size in physical pixels.
pub(super) struct Opened {
    window: DesktopWindow,
    output: OutputId,
    size: (u32, u32),
}

impl Editor {
    /// The floating windows the dock wants on the desktop. Open a window for
    /// each one not open yet, with an output of the renderer, and hand it
    /// over with [`open_desktop_window`](Self::open_desktop_window); close
    /// any open one that is no longer here, after
    /// [`forget_desktop_window`](Self::forget_desktop_window).
    pub fn wanted_windows(&mut self) -> Vec<DockWindow> {
        self.ui.dock_windows(self.nodes.dock)
    }

    /// A desktop window was opened for `window`, `size` physical pixels,
    /// drawn into `output`.
    pub fn open_desktop_window(&mut self, window: DesktopWindow, output: OutputId, size: (u32, u32)) {
        self.desktop.retain(|opened| opened.window != window);
        self.desktop.push(Opened { window, output, size });
        self.resize_desktop_window(window, size);
    }

    /// The desktop resized a desktop window, to `size` physical pixels.
    pub fn resize_desktop_window(&mut self, window: DesktopWindow, size: (u32, u32)) {
        if let Some(opened) = self.desktop.iter_mut().find(|opened| opened.window == window) {
            opened.size = size;
        }
        let logical = (size.0 as f32 / self.scale, size.1 as f32 / self.scale);
        self.ui.set_dock_window_size(self.nodes.dock, window, logical);
    }

    /// Where the desktop put a desktop window, in physical pixels, where it
    /// says, so it opens there again.
    pub fn place_desktop_window(&mut self, window: DesktopWindow, at: Option<(i32, i32)>) {
        self.ui.set_dock_window_place(self.nodes.dock, window, at);
    }

    /// The desktop window was closed from its own frame: its panels close, as
    /// Close on their menus does, and its window goes with the next
    /// [`wanted_windows`](Self::wanted_windows).
    pub fn close_desktop_window(&mut self, window: DesktopWindow) {
        let dock = self.nodes.dock;
        let Some(closed) = self.ui.dock_windows(dock).into_iter().find(|w| w.window == window) else { return };
        for panel in closed.panels {
            if !self.remove_opened_from_menu(&panel) {
                self.ui.close_dock_panel(dock, &panel);
            }
        }
    }

    /// No desktop window could be opened for `window`: its panels dock in
    /// the main window again rather than be lost.
    pub fn bring_back_desktop_window(&mut self, window: DesktopWindow) {
        let dock = self.nodes.dock;
        let Some(found) = self.ui.dock_windows(dock).into_iter().find(|w| w.window == window) else { return };
        for panel in found.panels {
            self.ui.dock_floating_panel(dock, &panel);
        }
    }

    /// Lets go of a desktop window the dock no longer wants, before it
    /// closes: returns the output it drew into, for the renderer to remove.
    pub fn forget_desktop_window(&mut self, window: DesktopWindow) -> Option<OutputId> {
        let index = self.desktop.iter().position(|opened| opened.window == window)?;
        if self.ui.pointer_window() == Some(window) {
            self.ui.set_pointer_window(None);
            self.ui.pointer_leave();
        }
        Some(self.desktop.remove(index).output)
    }

    /// The desktop window whose bar was pressed, to be moved by the desktop
    /// now, while the button is down. The press is then the desktop's.
    pub fn take_window_move(&mut self) -> Option<DesktopWindow> {
        let window = self.ui.take_window_move()?;
        self.ui.cancel_press();
        Some(window)
    }

    /// Gives the editor an event from a desktop window, or from the main
    /// window with `None`: the pointer's say which window it is in. Returns
    /// whether the editor used it.
    pub fn handle_in(&mut self, window: Option<DesktopWindow>, event: &winit::event::WindowEvent) -> bool {
        use winit::event::WindowEvent;
        let pointer = matches!(
            event,
            WindowEvent::CursorMoved { .. }
                | WindowEvent::CursorEntered { .. }
                | WindowEvent::MouseInput { .. }
                | WindowEvent::MouseWheel { .. }
                | WindowEvent::DroppedFile(_)
                | WindowEvent::HoveredFile(_)
        );
        // A window the pointer has already gone on from leaving it changes nothing.
        if matches!(event, WindowEvent::CursorLeft { .. }) && self.ui.pointer_window() != window {
            return false;
        }
        if pointer {
            self.ui.set_pointer_window(window);
        }
        self.handle(event)
    }

    /// The output `node` is drawn into, with its size in physical pixels:
    /// its desktop window's, or the main one's (`main`). `None` for one in a
    /// desktop window not open yet.
    pub(super) fn output_of(&self, node: NodeId, main: (u32, u32)) -> Option<(OutputId, (u32, u32))> {
        match self.ui.window_of(node) {
            None => Some((OutputId::MAIN, main)),
            Some(window) => {
                let opened = self.desktop.iter().find(|opened| opened.window == window)?;
                Some((opened.output, opened.size))
            }
        }
    }

    /// The outputs of the desktop windows open.
    pub(super) fn desktop_outputs(&self) -> Vec<OutputId> {
        self.desktop.iter().map(|opened| opened.output).collect()
    }

    /// Hands the renderer each desktop window's UI, after the main window's
    /// frame laid them out.
    pub(super) fn draw_desktop_windows(&mut self, renderer: &mut Renderer) -> anyhow::Result<()> {
        for opened in &self.desktop {
            let frame = self.ui.window_frame(opened.window);
            renderer.set_output_ui(opened.output, &frame)?;
        }
        Ok(())
    }
}
