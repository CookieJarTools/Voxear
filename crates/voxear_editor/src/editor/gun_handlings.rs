//! The Gun Editor's handling and the Objs its view shows (see `gun_editor`).
//! The handling open is written as it changes, each step once it is over,
//! and given before then to the guns that pick it, in the level edited or
//! the game playing, so a change shows at once. The view shows an instance
//! of the character picked, made for the editor alone far from the level,
//! holding its own gun, or a stand-in rifle when it has none, given the
//! handling open. It is made while the panel shows and the level is
//! edited, and taken away when the panel is hidden.

use anyhow::{Context, Result};
use voxear_assets::AssetKind;
use voxear_core::math::{Quat, Vec3};
use voxear_ecs::Entity;
use voxear_engine::{
    animation::{BoneAttachment, MotionMatcher},
    first_person::{FirstPersonState, FirstPersonView},
    guns::{
        self, Gun, GunHolder, GunKind,
        actions::{ActionId, Beat, BeatMark, Hand, expand_marked},
        handling::{CustomAction, GunHandling, put_handling},
        mechanism,
        playing::ActionPlay,
        preview::{HoldPreview, PreviewStart},
    },
    modules::{self, Module},
};
use voxear_render::{Camera, Renderer};
use voxear_scene::{EditorOnly, Name, Parent, Transform, WorldTransform, world_matrix};
use voxear_ui::NodeId;

use super::Editor;
use crate::{
    gun_details::{self, Button, DetailEdit, Field, Target},
    gun_editor::{self, Document, Preview, View},
    gun_timeline::TimelineEdit,
    hand_handles::{Grab, HandleEdit, Seen},
    objs,
    scene_tools::ViewCamera,
};

/// Where the character the Gun Editor shows stands: high over the level's
/// middle, out of its way, but near enough that its hands are drawn as
/// finely as anything in the level.
const SPOT: Vec3 = Vec3::new(0.0, 400.0, -1500.0);
/// What a Gun Handling's file starts with, above its poses and actions.
const HEADER: &str =
    "# Voxear Gun Handling: how a gun is held and how its actions go, as offsets on its kind's built-ins.\n";
/// How fast the character walks on the spot with Walk on, in metres a second.
const WALK_SPEED: f32 = 1.4;
/// How often the character menu looks for prefabs again, in frames.
const LOOK_FOR_CHARACTERS: u64 = 300;

impl Editor {
    /// The character the Gun Editor's view shows, its holder and the gun
    /// it holds, while it shows one.
    pub fn gun_preview(&self) -> Option<(Entity, Entity)> {
        self.gun_editor.preview.as_ref().map(|preview| (preview.holder, preview.gun))
    }

    /// Where the Gun Editor's view shows the handle on `hand`, or on the
    /// gun for none, in the window.
    pub fn gun_handle_at(&self, hand: Option<Hand>) -> Option<[f32; 2]> {
        let seen = self.gun_seen()?;
        let at = match hand {
            Some(hand) => seen.palms[hand as usize]?,
            None => seen.gun.w_axis.truncate(),
        };
        seen.camera.project(at)
    }

    /// The Gun Handling open in the Gun Editor, as it is now.
    pub fn gun_handling_open(&self) -> Option<&GunHandling> {
        self.gun_editor.doc.as_ref().map(|doc| &doc.handling)
    }

    /// Opens the Gun Editor on a Gun's handling, showing that Gun in the
    /// character's hands. A Gun with no handling is given a new one, as a
    /// step to undo.
    pub(super) fn open_gun_editor_on(&mut self, gun: Entity, renderer: &mut Renderer) {
        let Some(handling) = self.game.engine.world.get::<Gun>(gun).map(|gun| gun.handling.clone()) else { return };
        let found = self.game.content.parse(&handling).ok().and_then(|r| r.guid());
        let path = found.and_then(|guid| self.game.content.db().get(guid)).map(|entry| entry.path.clone());
        let path = match path {
            Some(path) => path,
            None => {
                let Some(path) = self.make_gun_handling("") else { return };
                let pick = crate::fields::Pick::Asset(Some(path.clone()));
                self.row_action_on(
                    gun,
                    crate::inspector::RowAction::Pick(crate::props::Prop::GunHandling, pick),
                    renderer,
                );
                path
            }
        };
        self.gun_editor.source = Some(gun);
        self.show_gun_handling(&path);
    }

    /// Opens the Gun Editor on the Gun Handling at `path`, the character
    /// holding its own gun.
    pub fn open_gun_editor(&mut self, path: &str) {
        self.gun_editor.source = None;
        self.show_gun_handling(path);
    }

    fn show_gun_handling(&mut self, path: &str) {
        match self.read_gun_handling(path) {
            Ok(doc) => {
                self.gun_editor.doc = Some(doc);
                (self.gun_editor.showing, self.gun_editor.running) = (None, false);
                self.memory.gun_editor.handling = Some(path.to_string());
                self.memory.save();
            }
            Err(error) => self.notify(&format!("Could not open {path}: {error:#}")),
        }
        self.show_panel("gun-editor");
    }

    fn read_gun_handling(&self, path: &str) -> Result<Document> {
        let db = self.game.content.db();
        let guid = db.guid_of(path).with_context(|| format!("{path} is not in the project"))?;
        let reference = self.game.content.parse(path)?.to_string();
        let text = std::fs::read_to_string(db.file(path))?;
        let handling: GunHandling = toml::from_str(&text)?;
        Ok(Document::new(path.to_string(), guid, reference, handling))
    }

    /// The Gun Editor, once a frame: what its bars ask for, the handling
    /// given to the guns picking it and written once a step is over, and the
    /// character in its view doing what the bars say.
    pub(super) fn update_gun_editor(&mut self, renderer: &mut Renderer, delta: f32) {
        self.give_open_handling();
        let shown = self.shown(self.gun_editor.viewport);
        if !shown {
            if let Some(doc) = self.gun_editor.doc.as_mut() {
                doc.hold(false);
            }
            self.write_open_handling();
            // While the game plays, the preview waits in the level as edited, which is put aside.
            if !self.is_playing() {
                self.end_gun_preview();
            }
            return;
        }
        if self.gun_editor.doc.is_none()
            && let Some(path) = self.memory.gun_editor.handling.clone()
            && let Ok(doc) = self.read_gun_handling(&path)
        {
            self.gun_editor.doc = Some(doc);
        }
        let frames = self.frames;
        if self.gun_editor.characters_looked.is_none_or(|at| frames >= at + LOOK_FOR_CHARACTERS) {
            self.gun_editor.characters_looked = Some(frames);
            let characters = self.gun_characters();
            let picked = self.memory.gun_editor.character.clone().or_else(|| characters.first().cloned());
            self.gun_editor.show_characters(&mut self.ui, characters, picked.as_deref());
        }
        self.gun_editor_bars();
        let has_guns = modules::has(&self.game.engine.world, Module::Guns);
        let why = if !has_guns {
            Some("Turn on the Guns module in Project Settings.")
        } else if self.is_playing() {
            Some("Shows while editing. Changes reach the game as they are made.")
        } else if self.gun_editor.doc.is_none() {
            Some("Pick a Gun Handling, or make one with +.")
        } else if self.gun_editor.character_picked(&self.ui).is_none() {
            Some("Pick a character: a prefab with a Gun Holder.")
        } else {
            None
        };
        if why.is_none() {
            self.keep_gun_preview(renderer);
        }
        let shown = self.shown_beats();
        let length = shown.as_ref().map(|(_, beats)| beats.iter().map(|(beat, _)| beat.length()).sum::<f32>());
        let speed = self.gun_editor.speed_picked(&self.ui);
        self.gun_editor.advance(delta, speed, length.unwrap_or(0.0));
        self.update_gun_timeline(shown.as_ref());
        self.update_gun_details(shown.as_ref().map(|(action, _)| action));
        if why.is_none() {
            self.update_gun_handles(shown.as_ref());
        }
        self.update_gun_problems();
        self.hold_open_handling();
        self.write_open_handling();
        if why.is_none() {
            self.drive_gun_preview();
        }
        let actions = self.gun_actions();
        self.gun_editor.show_actions(&mut self.ui, &actions);
        let why = why.or_else(|| self.gun_editor.preview.is_none().then_some("The character could not be made."));
        self.gun_editor.show_state(&mut self.ui, why, length);
    }

    /// The Gun Editor's view and the camera it looks through, while it
    /// shows the character.
    pub(super) fn gun_editor_view(&self) -> Option<(NodeId, Entity)> {
        let preview = self.gun_editor.preview.as_ref()?;
        let shown = self.shown(self.gun_editor.viewport) && !self.is_playing();
        shown.then_some((self.gun_editor.viewport, preview.camera))
    }

    /// Takes a step of the handling open back, or does it again, while the
    /// pointer is over the Gun Editor; false when it is elsewhere, for the
    /// level's own undo.
    pub(super) fn gun_editor_history(&mut self, undo: bool) -> bool {
        let over = self.ui.pointer_hovered().is_some_and(|node| self.is_in_gun_editor(node));
        let Some(doc) = self.gun_editor.doc.as_mut().filter(|_| over) else { return false };
        let done = if undo { doc.undo() } else { doc.redo() };
        if !done {
            self.notify(if undo { "Nothing to undo" } else { "Nothing to redo" });
        }
        true
    }

    fn is_in_gun_editor(&self, node: NodeId) -> bool {
        std::iter::successors(Some(node), |&n| self.ui.parent(n)).any(|n| n == self.gun_editor.panel)
    }

    /// Gives the guns picking the handling open the handling as it is now,
    /// before it is written: the character's in the view, and any in the
    /// level edited or the game playing.
    fn give_open_handling(&mut self) {
        let Some(doc) = &self.gun_editor.doc else { return };
        let world = &mut self.game.engine.world;
        let content = &self.game.content;
        let mut references: Vec<String> =
            world.query_ref::<&Gun>().into_iter().map(|gun| gun.handling.clone()).collect();
        references.sort();
        references.dedup();
        let picking = |reference: &String| {
            *reference == doc.reference || content.parse(reference).ok().and_then(|r| r.guid()) == Some(doc.guid)
        };
        for reference in references.iter().filter(|reference| picking(reference)) {
            put_handling(world, reference, doc.shared.clone());
        }
    }

    /// Writes the handling open once a step is over, and has the game read
    /// it again.
    fn write_open_handling(&mut self) {
        let Some(doc) = self.gun_editor.doc.as_mut().filter(|doc| doc.ready_to_write()) else { return };
        doc.unsaved = false;
        let (path, guid) = (doc.path.clone(), doc.guid);
        let written = toml::to_string(&doc.handling)
            .map_err(anyhow::Error::from)
            .and_then(|text| Ok(std::fs::write(self.game.content.db().file(&path), format!("{HEADER}{text}"))?));
        if let Err(error) = written {
            return self.notify(&format!("Could not save {path}: {error:#}"));
        }
        self.files_seen();
        self.game.content.forget_gun_handling(guid);
    }

    /// The prefabs with a Gun Holder at their top, the characters the Gun
    /// Editor can show, by path.
    fn gun_characters(&self) -> Vec<String> {
        let db = self.game.content.db();
        let holds =
            |path: &str| std::fs::read_to_string(db.file(path)).is_ok_and(|text| text.contains("[obj.gun_holder]"));
        let mut found: Vec<String> = db
            .entries()
            .filter(|entry| entry.kind == AssetKind::Prefab && holds(&entry.path))
            .map(|entry| entry.path.clone())
            .collect();
        found.sort();
        found
    }

    /// What the Gun Editor's bars were asked this frame.
    fn gun_editor_bars(&mut self) {
        let editor = &mut self.gun_editor;
        if self.ui.clicked(editor.handling) {
            self.browse_gun_handlings();
        }
        if self.ui.clicked(self.gun_editor.new) {
            self.new_gun_handling();
        }
        let editor = &mut self.gun_editor;
        if self.ui.changed(editor.character) {
            editor.failed = None;
            self.memory.gun_editor.character = editor.character_picked(&self.ui).map(str::to_string);
            self.memory.save();
        }
        if self.ui.changed(editor.view) {
            self.memory.gun_editor.view = editor.view_shown(&self.ui).index();
            self.memory.save();
        }
        if self.ui.clicked(editor.undo) || self.ui.clicked(editor.redo) {
            let undo = self.ui.clicked(editor.undo);
            if let Some(doc) = editor.doc.as_mut() {
                let _ = if undo { doc.undo() } else { doc.redo() };
            }
        }
        if let Some(id) = editor.action_clicked(&self.ui) {
            editor.play_action(id);
        }
        if self.ui.clicked(editor.play) {
            match (&editor.showing, editor.running) {
                (_, true) => editor.running = false,
                (Some(_), false) => {
                    // From its start again once it has played to the end.
                    let beats = self.shown_beats().map(|(_, beats)| beats).unwrap_or_default();
                    let length: f32 = beats.iter().map(|(beat, _)| beat.length()).sum();
                    let editor = &mut self.gun_editor;
                    if editor.time >= length {
                        editor.time = 0.0;
                    }
                    editor.running = true;
                }
                (None, false) => {
                    let first = self.gun_actions().into_iter().find(|(_, works)| *works).map(|(id, _)| id);
                    if let Some(id) = first {
                        self.gun_editor.play_action(id);
                    }
                }
            }
        }
        let editor = &mut self.gun_editor;
        if self.ui.clicked(editor.looping) {
            editor.looped = !editor.looped;
        }
        if self.ui.clicked(editor.fire) {
            self.fire_gun_preview();
        }
    }

    /// Opens the object picker on the project's Gun Handlings.
    fn browse_gun_handlings(&mut self) {
        let choices = crate::fields::assets_of(&self.game, Some(AssetKind::GunHandling));
        let choices = crate::object_picker::decorate(&self.game, choices);
        let now = self.gun_editor.doc.as_ref().map_or("None".to_string(), |doc| doc.path.clone());
        let target = crate::object_picker::PickFor::GunEditorHandling;
        self.picker.open(&mut self.ui, (target, self.gun_editor.handling), "Select Gun Handling", choices, &now);
    }

    /// Opens the handling picked in the Gun Editor's picker, or none.
    pub(super) fn pick_gun_editor_handling(&mut self, pick: &crate::fields::Pick) {
        match pick {
            crate::fields::Pick::Asset(Some(path)) => self.open_gun_editor(&path.clone()),
            crate::fields::Pick::Asset(None) => {
                self.gun_editor.doc = None;
                self.memory.gun_editor.handling = None;
                self.memory.save();
            }
            _ => {}
        }
    }

    /// Makes a new Gun Handling beside the character's prefab, or at the
    /// project's top, and opens it.
    fn new_gun_handling(&mut self) {
        let folder = self.gun_editor.character_picked(&self.ui).and_then(|path| path.rsplit_once('/')).map(|(f, _)| f);
        let folder = folder.unwrap_or("").to_string();
        if let Some(path) = self.make_gun_handling(&folder) {
            self.open_gun_editor(&path);
        }
    }

    /// Makes a new Gun Handling in `folder`, from the project's folder, and
    /// gives back its path once the project has it.
    fn make_gun_handling(&mut self, folder: &str) -> Option<String> {
        let root = self.game.project.root.clone();
        let made = crate::project_files::create(&root, folder, crate::project_files::NewFile::GunHandling, "");
        let path = match made {
            Ok(path) => path,
            Err(error) => {
                self.notify(&format!("Could not make a Gun Handling: {error:#}"));
                return None;
            }
        };
        if let Err(error) = self.game.content.rescan() {
            tracing::warn!("could not look at the project's files again: {error:#}");
        }
        self.files_seen();
        self.project.refresh(&mut self.ui, self.game.content.db(), Some(&path));
        Some(path)
    }

    /// Makes the character the Gun Editor shows, or makes it again when
    /// another is picked; gives its gun the handling open.
    fn keep_gun_preview(&mut self, renderer: &mut Renderer) {
        let Some(character) = self.gun_editor.character_picked(&self.ui).map(str::to_string) else { return };
        let world = &self.game.engine.world;
        // A level opened, or a prefab open on its own, may have taken its Objs away.
        let gone = self.gun_editor.preview.as_ref().is_some_and(|preview| !world.is_alive(preview.root));
        if gone {
            self.gun_editor.preview = None;
        }
        let source = self.gun_editor.source.filter(|&source| self.game.engine.world.has::<Gun>(source));
        let other = |preview: &Preview| preview.character != character || preview.source != source;
        if self.gun_editor.preview.as_ref().is_some_and(other) {
            self.end_gun_preview();
        }
        if self.gun_editor.preview.is_none() {
            // One that could not be made is not tried again every frame, only once picked again.
            if self.gun_editor.failed.as_deref() == Some(character.as_str()) {
                return;
            }
            match self.start_gun_preview(&character, renderer) {
                Ok(preview) => (self.gun_editor.preview, self.gun_editor.failed) = (Some(preview), None),
                Err(error) => {
                    tracing::warn!("the Gun Editor could not show {character}: {error:#}");
                    self.gun_editor.failed = Some(character);
                    return;
                }
            }
        }
        let Some(doc) = &self.gun_editor.doc else { return };
        let Some(preview) = self.gun_editor.preview.as_mut() else { return };
        if preview.handling != doc.path {
            preview.handling = doc.path.clone();
            if let Some(gun) = self.game.engine.world.get_mut::<Gun>(preview.gun) {
                gun.handling = doc.reference.clone();
            }
        }
    }

    /// Makes an instance of `character` far from the level, with a camera
    /// of its own, holding its gun or a stand-in rifle.
    fn start_gun_preview(&mut self, character: &str, renderer: &mut Renderer) -> Result<Preview> {
        let root = objs::prefab_editor_only(&mut self.game, Some(renderer), character)?;
        let facing = Quat::IDENTITY;
        let world = &mut self.game.engine.world;
        if let Some(transform) = world.get_mut::<Transform>(root) {
            (transform.position, transform.rotation) = (SPOT, facing);
        }
        let family = objs::family(world, root);
        let Some(holder) = family.iter().copied().find(|&e| world.has::<GunHolder>(e)) else {
            for entity in family {
                world.despawn(entity);
            }
            anyhow::bail!("it has no Gun Holder");
        };
        let lens = family.iter().find_map(|&e| world.get::<Camera>(e).copied()).unwrap_or_default();
        let head_hiders =
            family.iter().copied().filter(|&e| world.get::<BoneAttachment>(e).is_some_and(|a| a.hide)).collect();
        let own = family.iter().copied().find(|&e| world.has::<Gun>(e));
        // The Gun the editor was opened from takes the place of the character's own.
        let source = self.gun_editor.source.filter(|&source| self.game.engine.world.has::<Gun>(source));
        let gun = match (source, own) {
            (Some(source), own) => {
                let copy = objs::copy_editor_only(&mut self.game, Some(renderer), source)?;
                let world = &mut self.game.engine.world;
                world.insert(copy, Parent(holder));
                for entity in own.map(|own| objs::family(world, own)).unwrap_or_default() {
                    world.despawn(entity);
                }
                copy
            }
            (None, Some(gun)) => gun,
            (None, None) => {
                let made = objs::spawn_editor_only(&mut self.game, Some(renderer), &objs::rifle_text())?;
                let rifle = *made.first().context("the rifle was made")?;
                self.game.engine.world.insert(rifle, Parent(holder));
                rifle
            }
        };
        let world = &mut self.game.engine.world;
        let lens = Camera { near: lens.near.min(0.02), ..lens };
        let camera =
            world.spawn((Name("Gun Editor Camera".into()), Transform::new(SPOT, facing, Vec3::ONE), lens, EditorOnly));
        world.insert(holder, HoldPreview::default());
        if let Some(holder) = world.get_mut::<GunHolder>(holder) {
            holder.wall_check = false;
        }
        let handling = String::new();
        let character = character.to_string();
        Ok(Preview { character, source, handling, root, holder, gun, camera, head_hiders, spot: SPOT, facing })
    }

    /// Takes away what the Gun Editor's view shows.
    fn end_gun_preview(&mut self) {
        let Some(preview) = self.gun_editor.preview.take() else { return };
        let world = &mut self.game.engine.world;
        for entity in objs::family(world, preview.root).into_iter().chain([preview.camera]) {
            if world.is_alive(entity) {
                world.despawn(entity);
            }
        }
    }

    /// Has the character do what the bars say, and puts the view's camera
    /// where the view picked looks from.
    fn drive_gun_preview(&mut self) {
        let Some(preview) = &self.gun_editor.preview else { return };
        let (holder, root, camera, spot, facing) =
            (preview.holder, preview.root, preview.camera, preview.spot, preview.facing);
        let hiders = preview.head_hiders.clone();
        let editor = &mut self.gun_editor;
        let (aiming, walking, start) = (editor.aiming(&self.ui), editor.walking(&self.ui), editor.start(&self.ui));
        let view = editor.view_shown(&self.ui);
        if view == View::Orbit && !editor.hands.is_held() {
            editor.orbit.steer(&self.ui, editor.viewport);
        }
        let at = editor.showing.clone().map(|id| (id, editor.time));
        let orbit = editor.orbit;
        let world = &mut self.game.engine.world;
        world.insert(holder, HoldPreview { start, at });
        if let Some(holder) = world.get_mut::<GunHolder>(holder) {
            holder.aiming = aiming;
        }
        // It stays where it stands, walking or not: its clips do not carry it.
        if let Some(transform) = world.get_mut::<Transform>(root) {
            (transform.position, transform.rotation) = (spot, facing);
        }
        let bodies: Vec<Entity> =
            objs::family(world, root).into_iter().filter(|&e| world.has::<MotionMatcher>(e)).collect();
        for body in bodies {
            if let Some(matcher) = world.get_mut::<MotionMatcher>(body) {
                matcher.root_motion = false;
                matcher.wanted_velocity = Some(if walking { facing * Vec3::Z * WALK_SPEED } else { Vec3::ZERO });
            }
        }
        let first_person = view == View::FirstPerson;
        if let Some(view) = world.get_mut::<FirstPersonView>(holder) {
            view.camera = first_person.then_some(camera);
            view.hide_head = first_person;
        }
        for hider in hiders {
            if let Some(attachment) = world.get_mut::<BoneAttachment>(hider) {
                attachment.hide = first_person;
            }
        }
        if first_person {
            return;
        }
        let gun = world_matrix(world, self.gun_editor.preview.as_ref().map_or(holder, |p| p.gun));
        let (position, rotation) = match view {
            View::Orbit => orbit.camera(gun.w_axis.truncate(), facing),
            _ => gun_editor::fixed_camera(view, gun, facing),
        };
        if let Some(transform) = world.get_mut::<Transform>(camera) {
            (transform.position, transform.rotation) = (position, rotation);
        }
        world.insert(camera, WorldTransform(voxear_core::math::Mat4::from_rotation_translation(rotation, position)));
    }

    /// Fires the character's gun, from the ammo state picked.
    fn fire_gun_preview(&mut self) {
        let Some(preview) = &self.gun_editor.preview else { return };
        let (holder, gun) = (preview.holder, preview.gun);
        let start = self.gun_editor.start(&self.ui);
        (self.gun_editor.showing, self.gun_editor.running) = (None, false);
        let world = &mut self.game.engine.world;
        world.insert(holder, HoldPreview { start, at: None });
        let Some(kind) = world.get::<Gun>(gun).map(|gun| gun.kind) else { return };
        let ready = mechanism::state_of(world, gun).is_some_and(|state| state.ready(kind));
        if !ready && let Some(now) = mechanism::state_of(world, gun) {
            world.insert(gun, start.state(kind, &now));
        }
        guns::fire(world, holder);
    }

    /// The actions the gun shown has, each with whether it does anything
    /// from the ammo state picked.
    fn gun_actions(&mut self) -> Vec<(ActionId, bool)> {
        let Some(doc) = &self.gun_editor.doc else { return Vec::new() };
        let handling = doc.handling.clone();
        let start = self.gun_editor.start(&self.ui);
        let ids = gun_editor::actions_of(&handling);
        let Some((kind, state, spare)) = self.preview_gun_state(start) else {
            return ids.into_iter().map(|id| (id, true)).collect();
        };
        let works = |id: &ActionId| ActionPlay::start(id, kind, &handling, &state, spare).is_some();
        ids.into_iter().map(|id| (id.clone(), works(&id))).collect()
    }

    /// The gun shown's kind, its state as actions start from it, and the
    /// rounds its holder has; with no character shown, a gun of the kind
    /// the handling was made for, full, and a holder's usual rounds.
    fn preview_gun_state(&mut self, start: PreviewStart) -> Option<(GunKind, mechanism::GunState, u32)> {
        let Some(preview) = self.gun_editor.preview.as_ref() else {
            let kind = self.gun_editor.doc.as_ref()?.handling.kind.unwrap_or(GunKind::BoltActionRifle);
            let full = mechanism::GunState::loaded(kind);
            return Some((kind, start.state(kind, &full), GunHolder::default().spare));
        };
        let world = &mut self.game.engine.world;
        let kind = world.get::<Gun>(preview.gun)?.kind;
        let spare = world.get::<GunHolder>(preview.holder)?.spare;
        let now = mechanism::state_of(world, preview.gun)?;
        Some((kind, start.state(kind, &now), spare))
    }

    /// The action showing as the details edit it, a built-in one's offsets
    /// or the action of the gun's own playing in its place, and the beats it
    /// plays from the ammo state picked, each with where it came from.
    fn shown_beats(&mut self) -> Option<(Target, Vec<(Beat, BeatMark)>)> {
        let id = self.gun_editor.showing.clone()?;
        let (kind, state, spare) = self.preview_gun_state(self.gun_editor.start(&self.ui))?;
        let handling = &self.gun_editor.doc.as_ref()?.handling;
        let own = |name: &str| handling.actions.iter().position(|own| own.name == name);
        let (target, spec) = match &id {
            ActionId::Builtin(action) => match handling.actions.iter().position(|own| own.replaces == Some(*action)) {
                Some(at) => (Target::Own(at, None), handling.actions[at].spec()),
                None => (Target::Builtin(*action, None), handling.spec(kind, *action)),
            },
            ActionId::Own(name) => {
                let at = own(name)?;
                (Target::Own(at, None), handling.actions[at].spec())
            }
        };
        Some((target, expand_marked(&spec, kind, &state, spare)))
    }

    /// The kind of gun the details edit for.
    fn gun_editor_kind(&mut self) -> GunKind {
        self.preview_gun_state(PreviewStart::Full).map_or(GunKind::BoltActionRifle, |(kind, ..)| kind)
    }

    /// The timeline: the action showing's beats drawn, and what dragging on
    /// it asks done.
    fn update_gun_timeline(&mut self, shown: Option<&(Target, Vec<(Beat, BeatMark)>)>) {
        let empty = Vec::new();
        let (action, beats) = shown.map_or((None, &empty), |(action, beats)| (Some(action), beats));
        let target_of = |at: usize| -> Option<Target> {
            let (beat, mark) = beats.get(at)?;
            match action? {
                Target::Builtin(kind, _) => Some(Target::Builtin(*kind, Some(beat.key))),
                Target::Own(own, _) => Some(Target::Own(*own, Some(mark.index))),
                Target::Poses => None,
            }
        };
        let kind = self.gun_editor_kind();
        for edit in self.gun_editor.timeline.drag(&self.ui, beats) {
            let editor = &mut self.gun_editor;
            match edit {
                TimelineEdit::Seek(time) => (editor.time, editor.running) = (time, false),
                TimelineEdit::Pick(at) => editor.picked = at.and_then(target_of),
                TimelineEdit::Retime { at, length } => {
                    let (Some(target), Some(doc)) = (target_of(at), editor.doc.as_mut()) else { continue };
                    doc.edit(|handling| gun_details::retime(handling, kind, &target, length));
                }
                TimelineEdit::Shift { at, by } => {
                    let (Some(target), Some(doc)) = (target_of(at), editor.doc.as_mut()) else { continue };
                    let from =
                        *editor.shift_from.get_or_insert_with(|| gun_details::start_of(&doc.handling, kind, &target));
                    doc.edit(|handling| gun_details::shift(handling, kind, &target, from + by));
                }
                TimelineEdit::Done => editor.shift_from = None,
            }
        }
        let editor = &mut self.gun_editor;
        let picked = editor.picked.clone();
        let picked_at = (0..beats.len()).find(|&at| target_of(at) == picked && picked.is_some());
        let edited: Vec<bool> = match (action, editor.doc.as_ref()) {
            (Some(Target::Builtin(kind, _)), Some(doc)) => {
                let offsets = doc.handling.offsets(*kind);
                let slot = |key| voxear_engine::guns::handling::BeatSlot::Known(key);
                beats
                    .iter()
                    .map(|(beat, _)| offsets.is_some_and(|o| o.beats.iter().any(|b| b.beat == slot(beat.key))))
                    .collect()
            }
            _ => vec![false; beats.len()],
        };
        let time = editor.time;
        editor.timeline.draw(&mut self.ui, beats, time, picked_at, &edited);
    }

    /// The details of what is picked: shown, and what was changed in them
    /// put in the handling.
    fn update_gun_details(&mut self, action: Option<&Target>) {
        let kind = self.gun_editor_kind();
        let editor = &mut self.gun_editor;
        let Some(doc) = editor.doc.as_mut() else { return };
        // A beat picked that the handling no longer has, as after an undo, is let go.
        let target = editor.picked.clone().filter(|picked| exists(&doc.handling, picked));
        let target = target.or_else(|| action.cloned()).unwrap_or(Target::Poses);
        editor.details.show(&mut self.ui, &doc.handling, kind, &target);
        for edit in editor.details.edits(&self.ui) {
            match edit {
                DetailEdit::Set(field, value) => {
                    doc.edit(|handling| gun_details::set(handling, kind, &target, field, value))
                }
                DetailEdit::Reset(field) => doc.edit(|handling| gun_details::reset(handling, kind, &target, field)),
                DetailEdit::Press(button) => {
                    let (picked, stop) = press(doc, kind, &target, button);
                    editor.picked = picked;
                    if stop {
                        (editor.showing, editor.running) = (None, false);
                    }
                }
            }
        }
        if self.ui.clicked(editor.add_action) {
            let name = (1..).map(|n| format!("Action {n}")).find(|name| doc.handling.action(name).is_none());
            let name = name.expect("some name is free");
            doc.edit(|handling| {
                let beats = vec![Beat::default()];
                handling.actions.push(CustomAction { name: name.clone(), beats, ..CustomAction::default() });
            });
            editor.play_action(ActionId::Own(name.as_str().into()));
        }
    }

    /// The view's handles: drawn on the hands and the gun, and what
    /// dragging them changes put in the handling. A hand working in the
    /// beat showing moves the beat's hold on its goal; at rest it moves the
    /// handling's offset on its socket. The gun moves where it is held for
    /// the action showing, or at the eye or the hip.
    fn update_gun_handles(&mut self, shown: Option<&(Target, Vec<(Beat, BeatMark)>)>) {
        let Some(seen) = self.gun_seen() else { return };
        let kind = self.gun_editor_kind();
        let editor = &mut self.gun_editor;
        let (time, aiming) = (editor.time, editor.aiming(&self.ui));
        let target_of = |grab: Grab| grab_target(grab, shown, time, aiming);
        let Some(handling) = editor.doc.as_ref().map(|doc| doc.handling.clone()) else { return };
        let value = |grab: Grab| {
            let (target, field) = target_of(grab);
            match gun_details::value(&handling, kind, &target, field) {
                gun_details::Value::Vector(v) => Some(v),
                _ => None,
            }
        };
        let layers = (editor.viewport, editor.handles);
        let edit = editor.hands.update(&mut self.ui, layers, &seen, self.alt, value);
        let Some(doc) = editor.doc.as_mut() else { return };
        if let Some(HandleEdit::Moved(grab, moved)) = edit {
            let (target, field) = target_of(grab);
            doc.hold(true);
            doc.edit(|handling| gun_details::set(handling, kind, &target, field, gun_details::Value::Vector(moved)));
        }
    }

    /// What the view's handles are drawn from: its camera, the gun, its
    /// surfaces and the character's palms.
    fn gun_seen(&self) -> Option<Seen> {
        let preview = self.gun_editor.preview.as_ref()?;
        let world = &self.game.engine.world;
        let camera = ViewCamera::new(world, preview.camera, self.ui.rect(self.gun_editor.viewport))?;
        let gun = world_matrix(world, preview.gun);
        let shapes = guns::contact::shapes_now(world, preview.gun, &[]);
        let eye = world.get::<FirstPersonState>(preview.holder)?.eye;
        let body = world.get::<FirstPersonView>(preview.holder).and_then(|view| view.body);
        let body = body.or_else(|| {
            objs::family(world, preview.holder).into_iter().find(|&e| world.has::<voxear_render::ModelRenderer>(e))
        });
        let palms = body.and_then(|body| palms(world, body)).unwrap_or([None; 2]);
        Some(Seen { camera, gun, shapes, palms, eye })
    }

    /// The strip of what is wrong: parts the gun's kind is worked by that
    /// it has no Obj for, a hand in the gun, sights off the eye's line while
    /// aiming, and offsets on beats the built-ins no longer have, which a
    /// button there takes away.
    fn update_gun_problems(&mut self) {
        let kind = self.gun_editor_kind();
        let mut problems = Vec::new();
        if let Some(preview) = self.gun_editor.preview.as_ref() {
            let (holder, gun_entity) = (preview.holder, preview.gun);
            let world = &self.game.engine.world;
            for part in mechanism::missing_parts(world, gun_entity, kind) {
                problems.push(format!("No {} part", crate::gun_timeline::part_title(part)));
            }
            if let Some(seen) = self.gun_seen() {
                let into = seen.gun.inverse();
                for (hand, palm) in ["Left", "Right"].into_iter().zip(seen.palms) {
                    let inside = palm
                        .is_some_and(|palm| guns::contact::nearest(&seen.shapes, into.transform_point3(palm)) == 0.0);
                    if inside {
                        problems.push(format!("{hand} hand in the gun"));
                    }
                }
                let aimed = world.get::<guns::GunHolderState>(holder).is_some_and(|state| state.aimed() > 0.97);
                let gun = world.get::<Gun>(gun_entity);
                let off =
                    gun.map(|gun| (seen.eye.inverse() * seen.gun * gun.sight_frame()).w_axis.truncate().truncate());
                if aimed && off.is_some_and(|off| off.length() > 0.01) {
                    problems.push("Sights off the eye line".to_string());
                }
            }
        }
        let editor = &mut self.gun_editor;
        let remove = editor.remove_stale_clicked(&self.ui);
        let Some(doc) = editor.doc.as_mut() else { return editor.show_problems(&mut self.ui, &problems, false) };
        let stale = doc.handling.stale(kind);
        for (action, beat) in &stale {
            let beat = match beat {
                voxear_engine::guns::handling::BeatSlot::Known(key) => key.title().to_string(),
                voxear_engine::guns::handling::BeatSlot::Retired(name) => name.clone(),
            };
            problems.push(format!("Offset on {} · {beat} does nothing", action.title()));
        }
        if remove {
            doc.edit(|handling| {
                for (action, beat) in &stale {
                    for offsets in handling.offsets.iter_mut().filter(|offsets| offsets.action == *action) {
                        offsets.beats.retain(|by| by.beat != *beat);
                    }
                }
                let empty = |o: &voxear_engine::guns::handling::ActionOffsets| {
                    *o == voxear_engine::guns::handling::ActionOffsets { action: o.action, ..Default::default() }
                };
                handling.offsets.retain(|offsets| !empty(offsets));
            });
        }
        editor.show_problems(&mut self.ui, &problems, !stale.is_empty());
    }

    /// Holds the handling open while a field is typed in or dragged, or
    /// the timeline dragged, so each is one step, written once let go.
    fn hold_open_handling(&mut self) {
        let editor = &mut self.gun_editor;
        let held = self.ui.is_scrubbing()
            || editor.details.typing(&self.ui)
            || editor.timeline.is_dragging()
            || editor.hands.is_held();
        if let Some(doc) = editor.doc.as_mut() {
            doc.hold(held);
        }
    }
}

/// Whether the handling has what `target` is of.
fn exists(handling: &GunHandling, target: &Target) -> bool {
    match target {
        Target::Own(action, beat) => {
            handling.actions.get(*action).is_some_and(|own| beat.is_none_or(|beat| beat < own.beats.len()))
        }
        _ => true,
    }
}

/// Does what a button of the details does to the handling. Gives back the
/// beat to pick after, and whether what shows should stop, its action gone.
fn press(doc: &mut Document, kind: GunKind, target: &Target, button: Button) -> (Option<Target>, bool) {
    match (button, target) {
        (Button::EditableCopy, Target::Builtin(action, _)) => {
            doc.edit(|handling| {
                let copy = handling.editable_copy(kind, *action);
                handling.offsets.retain(|offsets| offsets.action != *action);
                handling.actions.push(copy);
            });
            (None, false)
        }
        (Button::ResetAction, Target::Builtin(action, _)) => {
            doc.edit(|handling| handling.offsets.retain(|offsets| offsets.action != *action));
            (None, false)
        }
        (Button::AddBeat, Target::Own(action, _)) => {
            let mut added = None;
            doc.edit(|handling| {
                if let Some(own) = handling.actions.get_mut(*action) {
                    own.beats.push(Beat::default());
                    added = Some(own.beats.len() - 1);
                }
            });
            (added.map(|beat| Target::Own(*action, Some(beat))), false)
        }
        (Button::DeleteAction, Target::Own(action, _)) => {
            doc.edit(|handling| drop(handling.actions.remove(*action)));
            (None, true)
        }
        (Button::DuplicateBeat, Target::Own(action, Some(beat))) => {
            doc.edit(|handling| {
                if let Some(own) = handling.actions.get_mut(*action) {
                    own.beats.insert(*beat + 1, own.beats[*beat]);
                }
            });
            (Some(Target::Own(*action, Some(beat + 1))), false)
        }
        (Button::DeleteBeat, Target::Own(action, Some(beat))) => {
            doc.edit(|handling| {
                if let Some(own) = handling.actions.get_mut(*action) {
                    own.beats.remove(*beat);
                }
            });
            (None, false)
        }
        (Button::Earlier | Button::Later, Target::Own(action, Some(beat))) => {
            let to = if button == Button::Earlier { beat.checked_sub(1) } else { Some(beat + 1) };
            let count = doc.handling.actions.get(*action).map_or(0, |own| own.beats.len());
            let Some(to) = to.filter(|&to| to < count) else { return (Some(target.clone()), false) };
            doc.edit(|handling| handling.actions[*action].beats.swap(*beat, to));
            (Some(Target::Own(*action, Some(to))), false)
        }
        _ => (Some(target.clone()), false),
    }
}

/// What dragging a handle changes: a hand working in the beat showing at
/// `time`, the beat's hold on its goal; a hand at rest, the handling's
/// offset on its socket; the gun, where the action showing brings it, or
/// where it is held at the eye or the hip.
fn grab_target(
    grab: Grab,
    shown: Option<&(Target, Vec<(Beat, BeatMark)>)>,
    time: f32,
    aiming: bool,
) -> (Target, Field) {
    let working = shown.and_then(|(action, beats)| {
        let mut start = 0.0;
        let now = beats.iter().find(|(beat, _)| {
            start += beat.length();
            time < start
        });
        now.map(|(beat, mark)| (action, beat, mark))
    });
    match (grab, working) {
        (Grab::Hand(hand), Some((action, beat, mark))) if beat.hand == hand => match action {
            Target::Builtin(kind, _) => (Target::Builtin(*kind, Some(beat.key)), Field::HandAt),
            Target::Own(own, _) => (Target::Own(*own, Some(mark.index)), Field::HandAt),
            Target::Poses => (Target::Poses, Field::GripHand),
        },
        (Grab::Hand(Hand::Right), _) => (Target::Poses, Field::GripHand),
        (Grab::Hand(Hand::Left), _) => (Target::Poses, Field::SupportHand),
        (Grab::Gun, Some((action, ..))) => match action {
            Target::Builtin(kind, _) => (Target::Builtin(*kind, None), Field::Work),
            Target::Own(own, _) => (Target::Own(*own, None), Field::Work),
            Target::Poses => (Target::Poses, Field::Work),
        },
        (Grab::Gun, None) if aiming => (Target::Poses, Field::Aim),
        (Grab::Gun, None) => (Target::Poses, Field::Hip),
    }
}

/// Each hand's palm, left then right, in the world, as `body` is posed.
fn palms(world: &voxear_ecs::World, body: Entity) -> Option<[Option<Vec3>; 2]> {
    let animation = world.get::<voxear_render::ModelRenderer>(body)?.animation.clone()?;
    let rig = voxear_engine::animation::rig::Rig::find(&animation.skeleton)?;
    let pose = world.get::<voxear_render::Pose>(body)?;
    let edit = voxear_engine::animation::ik::PoseEdit::new(&animation.skeleton, &pose.bones);
    let placed = world_matrix(world, body);
    Some(
        rig.arms.each_ref().map(|arm| guns::hands::palm_of(&edit, arm).map(|palm| placed.transform_point3(palm.point))),
    )
}
