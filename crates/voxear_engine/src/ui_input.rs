//! Window input for Voxear UI: winit's events turned into the UI's pointer,
//! wheel, keyboard, text and input-method calls, and the system clipboard.
//! The overlay uses it today; the editor and in-game UI will too.

use voxear_ui::{Clipboard, Key, Modifiers, Ui};
use winit::{
    event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent},
    keyboard::{Key as WinitKey, NamedKey},
};

/// Pixels the wheel scrolls per notch.
const NOTCH: f32 = 40.0;

/// Hands a window event to `ui`. Returns whether the UI used it, so the
/// game can leave it alone: a click or wheel over UI, or a key while a text
/// field has focus.
pub fn handle(ui: &mut Ui, event: &WindowEvent) -> bool {
    match event {
        WindowEvent::CursorMoved { position, .. } => {
            ui.pointer_move(position.x as f32, position.y as f32);
            false
        }
        WindowEvent::CursorLeft { .. } => {
            ui.pointer_leave();
            false
        }
        WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
            ui.pointer_button(*state == ElementState::Pressed)
        }
        WindowEvent::MouseInput { state, button: MouseButton::Right, .. } => {
            ui.pointer_secondary(*state == ElementState::Pressed)
        }
        WindowEvent::MouseWheel { delta, .. } => {
            let (dx, dy) = match delta {
                MouseScrollDelta::LineDelta(x, y) => (-x * NOTCH, -y * NOTCH),
                MouseScrollDelta::PixelDelta(position) => (-position.x as f32, -position.y as f32),
            };
            ui.pointer_scroll(dx, dy)
        }
        WindowEvent::ModifiersChanged(modifiers) => {
            let state = modifiers.state();
            // Command on macOS does what Control does elsewhere.
            ui.set_modifiers(Modifiers { shift: state.shift_key(), control: state.control_key() || state.super_key() });
            false
        }
        WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
            let used = key(&event.logical_key).is_some_and(|key| ui.key_down(key));
            // With Control held the text is a shortcut's control code, not something typed.
            let shortcut =
                ui.wants_keyboard() && event.text.as_deref().is_some_and(|t| t.chars().all(char::is_control));
            let typed = !shortcut && event.text.as_deref().is_some_and(|text| ui.text_input(text));
            used || typed
        }
        WindowEvent::Ime(Ime::Preedit(text, cursor)) => {
            ui.ime_preedit(text, *cursor);
            ui.wants_keyboard()
        }
        WindowEvent::Ime(Ime::Commit(text)) => {
            ui.ime_preedit("", None);
            ui.text_input(text)
        }
        _ => false,
    }
}

fn key(key: &WinitKey) -> Option<Key> {
    Some(match key {
        WinitKey::Named(named) => match named {
            NamedKey::ArrowLeft => Key::Left,
            NamedKey::ArrowRight => Key::Right,
            NamedKey::ArrowUp => Key::Up,
            NamedKey::ArrowDown => Key::Down,
            NamedKey::Home => Key::Home,
            NamedKey::End => Key::End,
            NamedKey::PageUp => Key::PageUp,
            NamedKey::PageDown => Key::PageDown,
            NamedKey::Backspace => Key::Backspace,
            NamedKey::Delete => Key::Delete,
            NamedKey::Enter => Key::Enter,
            NamedKey::Escape => Key::Escape,
            NamedKey::Tab => Key::Tab,
            NamedKey::Space => Key::Space,
            _ => return None,
        },
        WinitKey::Character(text) => Key::Char(text.chars().next()?.to_ascii_lowercase()),
        _ => return None,
    })
}

/// The system clipboard, opened on first use. Where it cannot be opened,
/// copied text stays inside the program.
#[derive(Default)]
pub struct SystemClipboard {
    system: Option<arboard::Clipboard>,
    tried: bool,
    local: Option<String>,
}

impl SystemClipboard {
    fn system(&mut self) -> Option<&mut arboard::Clipboard> {
        if !std::mem::replace(&mut self.tried, true) {
            match arboard::Clipboard::new() {
                Ok(clipboard) => self.system = Some(clipboard),
                Err(error) => tracing::warn!("the system clipboard is unavailable ({error}); copying stays in Voxear"),
            }
        }
        self.system.as_mut()
    }
}

impl Clipboard for SystemClipboard {
    fn read(&mut self) -> Option<String> {
        match self.system() {
            Some(system) => system.get_text().ok(),
            None => self.local.clone(),
        }
    }

    fn write(&mut self, text: &str) {
        match self.system() {
            Some(system) => {
                let _ = system.set_text(text);
            }
            None => self.local = Some(text.to_string()),
        }
    }
}
