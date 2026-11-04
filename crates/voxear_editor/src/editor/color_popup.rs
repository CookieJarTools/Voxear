//! The colour picker, opened from a colour's swatch: a Light's colour in the
//! Inspector, a material's Colour or Emission in the asset view or on a
//! VoxDocs page, the emission being HDR, so the picker adds its intensity,
//! or a colour in Settings, such as the theme's. It opens under the
//! swatch, what is picked shows at once, and an Obj's changes over the whole
//! visit are one step to undo. A press outside it, or Escape, closes it.

use voxear_ecs::Entity;
use voxear_render::Renderer;
use voxear_ui::{ColorPicker, NodeId, PickedColor, Ui};

use super::Editor;
use crate::{
    asset_inspector::MaterialColor,
    history::{Edit, ObjState},
    props::{self, Prop, Value},
    settings::{self, SettingValue},
};

/// How far the picker keeps from the window's edges.
const MARGIN: f32 = 8.0;

/// What the picker is open for.
enum Target {
    /// An Obj's colour, and the Obj as it was when the picker opened, for undo.
    Prop { entity: Entity, prop: Prop, before: Box<ObjState> },
    /// A colour of the material the Inspector shows.
    Material(MaterialColor),
    /// A colour of a material on a VoxDocs page, by the place its fields
    /// are in.
    PageMaterial(MaterialColor, NodeId),
    /// A colour setting, by its key.
    Setting(&'static str),
    /// A colour of lighting settings, as the Lighting page or a `.lighting`
    /// asset's Inspector shows them: their file, and where it is in it.
    Lighting(std::path::PathBuf, &'static [&'static str]),
}

pub(super) struct ColorPopup {
    panel: NodeId,
    title: NodeId,
    picker: ColorPicker,
    /// What it is open for, and the swatch it opened from.
    open: Option<(Target, NodeId)>,
}

impl ColorPopup {
    pub fn new(ui: &mut Ui, panel: NodeId, title: NodeId) -> Self {
        let picker = ColorPicker::new(ui, panel);
        ui.set_visible(panel, false);
        Self { panel, title, picker, open: None }
    }
}

impl Editor {
    /// Opens the picker for an Obj's colour, under its swatch.
    pub(super) fn edit_color(&mut self, entity: Entity, prop: Prop, swatch: NodeId) {
        let Some(Value::Color(srgb)) = props::get(&self.game, self.inspector.types(), entity, &prop) else { return };
        let Some(before) = ObjState::of(&self.game.engine.world, entity) else { return };
        let title = props::label(&prop);
        let target = Target::Prop { entity, prop, before: Box::new(before) };
        self.open_color(target, swatch, &title, (PickedColor::srgb(srgb), false));
    }

    /// Whether the colour picker is open.
    pub fn color_picker_open(&self) -> bool {
        self.color_popup.open.is_some()
    }

    fn open_color(&mut self, target: Target, swatch: NodeId, title: &str, (color, hdr): (PickedColor, bool)) {
        self.close_color();
        let popup = &mut self.color_popup;
        popup.picker.set(&mut self.ui, color, hdr);
        let title = if hdr { format!("{title} · HDR") } else { title.to_string() };
        self.ui.set_text(popup.title, &title);
        self.ui.set_visible(popup.panel, true);
        popup.open = Some((target, swatch));
    }

    /// Opens the picker from a material's swatch, follows it while open, and
    /// closes it on a press elsewhere.
    pub(super) fn update_color_popup(&mut self, renderer: &mut Renderer) {
        let clicked = self.inspector.asset_view().and_then(|view| view.color_clicked(&self.ui));
        if let Some((which, swatch, color)) = clicked {
            let title = if which == MaterialColor::Base { "Colour" } else { "Emission" };
            return self.open_color(Target::Material(which), swatch, title, (color, which == MaterialColor::Emission));
        }
        let on_page = self.live_fields.iter().find_map(|(place, view)| Some((*place, view.color_clicked(&self.ui)?)));
        if let Some((place, (which, swatch, color))) = on_page {
            let title = if which == MaterialColor::Base { "Colour" } else { "Emission" };
            let target = Target::PageMaterial(which, place);
            return self.open_color(target, swatch, title, (color, which == MaterialColor::Emission));
        }
        if let Some((key, swatch)) = self.settings_panel.color_clicked(&self.ui)
            && let Some(SettingValue::Color(hex)) = self.memory.settings.get(key)
        {
            let title = settings::info(key).map_or(key, |info| info.title);
            let srgb = [hex >> 16, hex >> 8, hex].map(|c| (c & 0xff) as f32 / 255.0);
            return self.open_color(Target::Setting(key), swatch, title, (PickedColor::srgb(srgb), false));
        }
        let lighting = self.lighting_panel.color_clicked(&self.ui);
        let clicked = lighting.or_else(|| self.inspector.asset_view().and_then(|view| view.lighting_color_clicked(&self.ui)));
        if let Some(clicked) = clicked {
            let title = clicked.path.last().copied().unwrap_or("Colour").replace('_', " ");
            let target = Target::Lighting(clicked.file, clicked.path);
            return self.open_color(target, clicked.swatch, &title, (PickedColor::srgb(clicked.srgb), false));
        }
        let Some(&(_, swatch)) = self.color_popup.open.as_ref() else { return };
        let (root, panel) = (self.ui.root(), self.color_popup.panel);
        if self.ui.is_pressed(root) && !self.ui.is_pressed(panel) && !self.ui.is_pressed(swatch) {
            return self.close_color();
        }
        self.place_color(swatch);
        let Some(color) = self.color_popup.picker.update(&mut self.ui) else { return };
        match &self.color_popup.open {
            Some((Target::Prop { entity, prop, .. }, _)) => {
                let (entity, prop) = (*entity, prop.clone());
                props::set(
                    &mut self.game,
                    Some(renderer),
                    self.inspector.types(),
                    entity,
                    &prop,
                    Value::Color(color.srgb),
                );
            }
            Some((Target::Material(which), _)) => {
                let which = *which;
                let action = self.inspector.asset_view().and_then(|view| view.set_color(&mut self.ui, which, color));
                if let Some(action) = action {
                    self.asset_action(action, renderer);
                }
            }
            Some((Target::PageMaterial(which, place), _)) => {
                let (which, place) = (*which, *place);
                let view = self.live_fields.iter_mut().find(|(at, _)| *at == place).map(|(_, view)| view);
                if let Some(action) = view.and_then(|view| view.set_color(&mut self.ui, which, color)) {
                    self.asset_action(action, renderer);
                }
            }
            Some((Target::Lighting(file, path), _)) => {
                let action = crate::lighting_panel::set_color(file.clone(), path, color.srgb);
                self.lighting_action(action, renderer);
            }
            Some((Target::Setting(key), _)) => {
                let [r, g, b] = color.srgb.map(|c| (c * 255.0).round() as u32);
                self.memory.settings.set(key, SettingValue::Color(r << 16 | g << 8 | b));
            }
            None => {}
        }
    }

    /// Closes the picker. What it changed on an Obj is one step to undo, and
    /// a setting is kept once picked rather than at every step of a drag.
    pub(super) fn close_color(&mut self) {
        let Some((target, _)) = self.color_popup.open.take() else { return };
        self.ui.set_visible(self.color_popup.panel, false);
        if let Target::Setting(_) = target {
            return self.memory.save();
        }
        let Target::Prop { entity, before, .. } = target else { return };
        let Some(after) = ObjState::of(&self.game.engine.world, entity) else { return };
        if after != *before && !self.is_playing() {
            self.history.record_step(Edit::Changed { entity, before: *before, after });
            self.unsaved = true;
        }
    }

    /// Under the swatch, kept inside the window: above it when there is no
    /// room below.
    fn place_color(&mut self, swatch: NodeId) {
        // Over the window the swatch is in, a desktop window of a panel floated out too.
        self.ui.show_over_window(self.color_popup.panel, self.ui.window_of(swatch));
        let [x, y, _, height] = self.ui.rect(swatch);
        let (window_width, window_height) = self.ui.screen_size(swatch);
        let [_, _, width, own_height] = self.ui.rect(self.color_popup.panel);
        let left = x.min(window_width - width - MARGIN).max(MARGIN);
        let below = y + height + 4.0;
        let top = if below + own_height > window_height - MARGIN { (y - own_height - 4.0).max(MARGIN) } else { below };
        self.ui.set_inline_style(self.color_popup.panel, &format!("left: {left}px; top: {top}px"));
    }
}
