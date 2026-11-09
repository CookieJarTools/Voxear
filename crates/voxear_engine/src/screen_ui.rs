//! Screen UIs: `.vui` documents a component shows over the game's view for
//! as long as its Obj is there, as a HUD or a menu, with no script to load
//! them. Each is a document of the game's screen UI (see
//! [`crate::game_ui`]), so scripts find and change its elements as those
//! of a document they loaded.
//!
//! A Screen UI's document loads the first time a script asks for it, as in
//! Awake or Start, or else after the frame's scripts, and closes when its
//! Obj goes. An Obj switched off hides it, keeping its elements and their
//! handlers. Screen UIs show over one another by their `order`, the highest
//! over all, and over documents scripts loaded before them. They show only
//! while the game plays.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use voxear_assets::AssetGuid;
use voxear_ecs::{Entity, World};
use voxear_scene::Destroying;

use crate::{Content, game_ui::GameUi};

/// A Screen UI: a `.vui` document shown over the game's view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScreenUi {
    /// The document it shows.
    pub document: Option<AssetGuid>,
    /// Which Screen UI shows over which: a higher order over a lower.
    pub order: i32,
}

impl voxear_ecs::Component for ScreenUi {}

/// The number scripts hold a Screen UI's document by, on its Obj while it
/// shows one, for `ScreenUi.Document` in scripts to read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenDocument(pub u32);

impl voxear_ecs::Component for ScreenDocument {}

/// A Screen UI as scenes write it. Left out, a setting has its default.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScreenUiData {
    /// The document, by GUID or project path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub order: i32,
}

fn is_zero(order: &i32) -> bool {
    *order == 0
}

impl ScreenUiData {
    /// The settings that differ from `expected`, each written out even at
    /// its default, as a prefab instance's override merges them onto the
    /// prefab's.
    pub fn changes_from(&self, expected: Option<&Self>) -> toml::Table {
        let table = |data: &Self| {
            let mut table = toml::Table::new();
            if let Some(document) = &data.document {
                table.insert("document".into(), document.clone().into());
            }
            table.insert("order".into(), i64::from(data.order).into());
            table
        };
        let mut changes = table(self);
        if let Some(expected) = expected.map(table) {
            changes.retain(|key, value| expected.get(key) != Some(value));
        }
        changes
    }
}

/// A Screen UI from a scene's data, its document found by GUID or path.
pub fn screen_ui(content: &Content, data: &ScreenUiData) -> Result<ScreenUi> {
    let document = match &data.document {
        Some(reference) => {
            let found = content.parse(reference)?;
            Some(found.guid().with_context(|| format!("`{reference}` is not a UI document"))?)
        }
        None => None,
    };
    Ok(ScreenUi { document, order: data.order })
}

/// What a scene writes for a Screen UI.
pub fn screen_ui_data(screen: &ScreenUi) -> ScreenUiData {
    ScreenUiData { document: screen.document.map(|guid| guid.to_string()), order: screen.order }
}

/// A Screen UI's document on the screen: its Obj, the document it shows,
/// by GUID and by the number scripts hold it by, and how it shows.
pub(crate) struct ScreenDoc {
    entity: Entity,
    shown: Option<AssetGuid>,
    document: Option<u32>,
    /// A document that would not load, not tried again until it changes.
    failed: Option<AssetGuid>,
    order: i32,
    hidden: bool,
}

impl GameUi {
    /// Keeps a document on the screen for each Screen UI in `world`: loads
    /// one for each new Screen UI and again when its document changes,
    /// closes those whose Objs went, hides those whose Objs are off, and
    /// puts them over one another by their order.
    pub fn sync_screens(&mut self, world: &mut World, content: &mut Content) {
        let found: Vec<(Entity, ScreenUi)> = world
            .query_ref::<(Entity, &ScreenUi)>()
            .without::<Destroying>()
            .into_iter()
            .map(|(entity, screen)| (entity, *screen))
            .collect();
        let gone: Vec<u32> = self
            .screens
            .iter()
            .filter(|kept| !found.iter().any(|(entity, _)| *entity == kept.entity))
            .filter_map(|kept| kept.document)
            .collect();
        for document in gone {
            self.screen.close(document);
        }
        self.screens.retain(|kept| found.iter().any(|(entity, _)| *entity == kept.entity));
        let switched = voxear_scene::Switched::of(world);
        let mut moved = false;
        for (entity, screen) in found {
            let before = self.screens.iter().find(|kept| kept.entity == entity).map(|kept| (kept.document, kept.order));
            let at = self.keep_screen(world, content, entity, &screen);
            let kept = &mut self.screens[at];
            moved |= before != Some((kept.document, kept.order));
            let hidden = !switched.is_active(entity);
            if let Some(document) = kept.document.filter(|_| kept.hidden != hidden) {
                self.screen.set_document_visible(document, !hidden);
            }
            kept.hidden = hidden;
        }
        if moved {
            self.order_screens();
        }
    }

    /// Loads the document `entity`'s Screen UI shows now, if it has not
    /// loaded yet, as when a script asks for it in Awake or Start. Gives
    /// back the number scripts hold it by.
    pub fn load_screen(&mut self, world: &mut World, content: &mut Content, entity: Entity) -> Option<u32> {
        let screen = *world.get::<ScreenUi>(entity)?;
        let new = !self.screens.iter().any(|kept| kept.entity == entity);
        let at = self.keep_screen(world, content, entity, &screen);
        if new {
            self.order_screens();
        }
        self.screens[at].document
    }

    /// The number scripts hold a Screen UI's document by, once it shows one.
    pub fn screen_document(&self, entity: Entity) -> Option<u32> {
        self.screens.iter().find(|kept| kept.entity == entity)?.document
    }

    /// Loads every document from a file again, as after files changed: the
    /// screen's, Screen UIs' and those scripts loaded, and each UI Panel's.
    /// Each keeps the numbers scripts hold its elements by, so their
    /// handlers keep running. One that would not load is tried again.
    pub fn reload_documents(&mut self, content: &mut Content) {
        self.screen.reload_files(content);
        for kept in &mut self.screens {
            kept.failed = None;
        }
        self.reload_panels(content);
    }

    /// Where `entity`'s Screen UI is kept, made if it has none, showing the
    /// document `screen` names, whose number its Obj holds for scripts.
    fn keep_screen(&mut self, world: &mut World, content: &mut Content, entity: Entity, screen: &ScreenUi) -> usize {
        let at = match self.screens.iter().position(|kept| kept.entity == entity) {
            Some(at) => at,
            None => {
                let made =
                    ScreenDoc { entity, shown: None, document: None, failed: None, order: screen.order, hidden: false };
                self.screens.push(made);
                self.screens.len() - 1
            }
        };
        self.screens[at].order = screen.order;
        let (shown, failed) = (self.screens[at].shown, self.screens[at].failed);
        if shown != screen.document && failed != screen.document {
            self.show_screen(at, content, screen.document);
        }
        let document = self.screens[at].document.map(ScreenDocument);
        if world.get::<ScreenDocument>(entity).copied() != document {
            match document {
                Some(document) => drop(world.insert(entity, document)),
                None => drop(world.remove::<ScreenDocument>(entity)),
            }
        }
        at
    }

    /// Shows `document` for the Screen UI kept at `at`, in place of what it
    /// showed, or nothing.
    fn show_screen(&mut self, at: usize, content: &mut Content, document: Option<AssetGuid>) {
        if let Some(number) = self.screens[at].document.take() {
            self.screen.close(number);
        }
        let kept = &mut self.screens[at];
        (kept.shown, kept.failed, kept.hidden) = (document, None, false);
        let Some(guid) = document else { return };
        let path = content.db().get(guid).map(|entry| entry.path.clone());
        let loaded = path
            .with_context(|| format!("no asset has the GUID {guid}"))
            .and_then(|path| self.screen.load(content, &path).with_context(|| format!("showing {path} on the screen")));
        let kept = &mut self.screens[at];
        match loaded {
            Ok(number) => kept.document = Some(number),
            Err(error) => {
                tracing::warn!("{error:#}");
                kept.shown = None;
                kept.failed = Some(guid);
            }
        }
    }

    /// Puts the Screen UIs' documents over one another by their order, the
    /// one made first under another of the same order.
    fn order_screens(&mut self) {
        let mut shown: Vec<(i32, usize, u32)> = self
            .screens
            .iter()
            .enumerate()
            .filter_map(|(made, kept)| Some((kept.order, made, kept.document?)))
            .collect();
        shown.sort();
        let documents: Vec<u32> = shown.into_iter().map(|(_, _, document)| document).collect();
        self.screen.raise(&documents);
    }
}

/// Sets what answers `ScreenUi` in scripts.
pub(crate) fn register() {
    voxear_scripting::set_kind_values(voxear_scripting::ComponentKind::ScreenUi, values);
}

// What `values` reads or writes, as `ScreenUi` in Voxear.Core numbers them.
const HAS: i32 = 0;
const ADD: i32 = 1;
const REMOVE: i32 = 2;
const ORDER: i32 = 3;
const DOCUMENT: i32 = 4;

/// Reads value `what` of `entity`'s Screen UI, or with `written`, writes
/// it: `None` when it has none, or is gone. Its document reads as the
/// number scripts hold it by, bit for bit, and whether it shows one.
fn values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    match (what, written) {
        (HAS, None) => Some([f32::from(u8::from(world.has::<ScreenUi>(entity))), 0.0, 0.0, 0.0]),
        (ADD, Some(_)) => {
            if !world.has::<ScreenUi>(entity) {
                world.insert(entity, ScreenUi::default());
            }
            Some([0.0; 4])
        }
        (REMOVE, Some(_)) => {
            world.remove::<ScreenDocument>(entity);
            world.remove::<ScreenUi>(entity).map(|_| [0.0; 4])
        }
        (ORDER, None) => Some([world.get::<ScreenUi>(entity)?.order as f32, 0.0, 0.0, 0.0]),
        (ORDER, Some([order, ..])) => {
            world.get_mut::<ScreenUi>(entity)?.order = order.round() as i32;
            Some([0.0; 4])
        }
        (DOCUMENT, None) => {
            world.get::<ScreenUi>(entity)?;
            // Loaded the first time a script asks, so it is there in Awake and Start.
            let loaded = voxear_scripting::ui_screen_document(world, entity);
            let document = loaded.or_else(|| world.get::<ScreenDocument>(entity).map(|document| document.0));
            Some([f32::from_bits(document.unwrap_or(0)), f32::from(u8::from(document.is_some())), 0.0, 0.0])
        }
        _ => None,
    }
}
