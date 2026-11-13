//! The function tables shared with `Voxear.Core`. Every struct here has a
//! mirror in `managed/Voxear.Core/Interop`; change both together and bump
//! [`API_VERSION`].
//!
//! Values behind pointers C# hands over are read and written unaligned. C#
//! aligns a struct of floats to 4 bytes, but glam's `Quat` (and so
//! `Transform`) asks for 16 on x86-64, and an aligned SIMD store to an
//! address C# placed otherwise crashes; a Release build of `Voxear.Core`
//! lays out its stack so that it does.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ptr,
    sync::atomic::Ordering,
};

use voxear_core::{
    profiling::{self, Marker},
    time::Time,
};
use voxear_ecs::{Entity, World};
use voxear_scene::{
    Adopted, AttachedBehaviour, BehaviourHandle, BehaviourSpec, Behaviours, Callbacks, InstanceMarker, Name, Placement,
    Registration, Request, SceneComponents, Transform, world_matrix,
};

use crate::audio::{PlaySound, SoundFacts, audio_bus, sound_info, sound_load, sound_play, source_value, voice_value};
use crate::debug::{debug_box, debug_line, debug_screen_text, debug_solid, debug_sphere, debug_text};
use crate::display;
use crate::ecs::{
    self, CURRENT_WORLD, QueryDesc, change_tick, chunk_written, commands_defer, component_add, component_get,
    component_has, component_register, component_remove, component_set, entity_alive, entity_create, entity_destroy,
    query_chunks,
};
use crate::saves::{save_change, save_info, save_read, save_wait, save_write};
use crate::services::{
    AssetName, ContentStats, LevelInfo, LevelLoadOptions, LoadProgress, PreviewOptions, PreviewSource, PreviewState,
    SceneLoadOptions, SceneRequest, ScriptServices, SpawnedPrefab, with_services,
};
use crate::ui::{
    ui_changed, ui_class, ui_clicked, ui_close, ui_create, ui_find, ui_items, ui_load, ui_new, ui_remove, ui_set_text,
    ui_style, ui_text, ui_value, ui_visible,
};

pub(crate) const API_VERSION: u32 = 84;

/// Engine functions scripts call.
#[repr(C)]
pub(crate) struct NativeApi {
    pub size: u32,
    pub version: u32,
    pub time: *const Time,
    /// A script's line, with its stack (a frame to a line), and what it is
    /// about: an Obj (0 for none), a loaded prefab (-1 for none), or an asset
    /// by its project path or GUID (empty for none).
    pub log: unsafe extern "C" fn(
        level: i32,
        message: *const u8,
        len: i32,
        stack: *const u8,
        stack_len: i32,
        context: u64,
        prefab: i32,
        asset: *const u8,
        asset_len: i32,
    ),
    pub obj_name: unsafe extern "C" fn(entity: u64, buffer: *mut u8, capacity: i32) -> i32,
    pub transform_get: unsafe extern "C" fn(entity: u64, out: *mut Transform) -> i32,
    pub transform_set: unsafe extern "C" fn(entity: u64, value: *const Transform) -> i32,
    pub behaviour_set_enabled: unsafe extern "C" fn(handle: u32, enabled: i32),
    pub behaviour_destroy: unsafe extern "C" fn(handle: u32),
    pub level_load: unsafe extern "C" fn(level: *const u8, len: i32),
    pub obj_destroy: unsafe extern "C" fn(entity: u64),
    pub obj_behaviours: unsafe extern "C" fn(entity: u64, out: *mut u32, capacity: i32) -> i32,
    pub behaviour_add: unsafe extern "C" fn(entity: u64, type_name: *const u8, len: i32) -> i32,
    pub prefab_load: unsafe extern "C" fn(reference: *const u8, len: i32) -> i32,
    /// `placement` is optional; only its position and rotation are used.
    pub prefab_instantiate: unsafe extern "C" fn(
        prefab: i32,
        placement: *const Transform,
        world_space: i32,
        parent: u64,
        out_root: *mut u64,
    ) -> i32,
    pub batch_handles: unsafe extern "C" fn(out: *mut u32, capacity: i32) -> i32,
    pub behaviours_adopt: unsafe extern "C" fn(handles: *const u32, states: *const u32, count: i32),
    pub obj_world_transform: unsafe extern "C" fn(entity: u64, out: *mut Transform) -> i32,
    pub load_async: unsafe extern "C" fn(level: i32, reference: *const u8, len: i32) -> i32,
    pub load_status: unsafe extern "C" fn(id: u32, out: *mut LoadProgress) -> i32,
    pub load_allow_activation: unsafe extern "C" fn(id: u32, allow: i32),
    pub prefab_keep: unsafe extern "C" fn(prefab: i32, keep: i32),
    pub content_unload_unused: unsafe extern "C" fn(),
    pub content_usage: unsafe extern "C" fn(out: *mut ContentStats) -> i32,
    pub level_load_async: unsafe extern "C" fn(reference: *const u8, len: i32, options: *const LevelLoadOptions) -> i32,
    pub level_info: unsafe extern "C" fn(what: i32, id: i32, buffer: *mut u8, capacity: i32) -> i32,
    /// Writes the Objs called `name`, oldest first, as many as fit, and
    /// returns how many there are.
    pub obj_find: unsafe extern "C" fn(name: *const u8, len: i32, out: *mut u64, capacity: i32) -> i32,
    /// Profiling, for `Voxear.Profiler` and each behaviour's callbacks: a
    /// marker by name, the same each time; a sample begun, 1 when it was
    /// recorded, as it is not while sampling is off; the last one ended; and
    /// whether sampling is on.
    pub profiler_marker: unsafe extern "C" fn(name: *const u8, len: i32) -> u32,
    pub profiler_begin: unsafe extern "C" fn(marker: u32) -> i32,
    pub profiler_end: unsafe extern "C" fn(),
    pub profiler_enabled: unsafe extern "C" fn() -> i32,
    /// Previews, for `Voxear.Preview`: one asked for of a loaded prefab (0
    /// or more) or else of an asset by reference, returning its id or -1;
    /// where one is drawn (1 when drawn, 0 while waiting, -1 for none); one
    /// saved to a PNG file once drawn (0 when it will be, -1 when not); and
    /// one let go.
    pub preview_create:
        unsafe extern "C" fn(prefab: i32, reference: *const u8, len: i32, options: *const PreviewOptions) -> i32,
    pub preview_state: unsafe extern "C" fn(preview: i32, out: *mut PreviewState) -> i32,
    pub preview_save: unsafe extern "C" fn(preview: i32, path: *const u8, len: i32) -> i32,
    pub preview_free: unsafe extern "C" fn(preview: i32),
    /// The ECS, for `Voxear.Entity`, `SystemBase` and queries (see
    /// [`crate::ecs`]): the id of the world lent to scripts now, read in
    /// place; components by name and layout; chunks a query matches; a
    /// fetched column marked written; entities and their components; the
    /// commands' wait; and the change tick.
    pub world_id: *const u64,
    pub component_register: unsafe extern "C" fn(name: *const u8, len: i32, size: i32, align: i32) -> i32,
    pub query_chunks: unsafe extern "C" fn(query: *const QueryDesc, out: *mut isize, capacity: i32) -> i32,
    pub chunk_written: unsafe extern "C" fn(place: u64, component: u32),
    pub entity_create: unsafe extern "C" fn() -> u64,
    pub entity_destroy: unsafe extern "C" fn(entity: u64),
    pub entity_alive: unsafe extern "C" fn(entity: u64) -> i32,
    pub component_has: unsafe extern "C" fn(entity: u64, component: u32) -> i32,
    pub component_get: unsafe extern "C" fn(entity: u64, component: u32, out: *mut u8, size: i32) -> i32,
    pub component_set: unsafe extern "C" fn(entity: u64, component: u32, value: *const u8, size: i32) -> i32,
    pub component_add: unsafe extern "C" fn(entity: u64, component: u32, value: *const u8, size: i32) -> i32,
    pub component_remove: unsafe extern "C" fn(entity: u64, component: u32) -> i32,
    pub commands_defer: unsafe extern "C" fn(begin: i32),
    pub change_tick: unsafe extern "C" fn(advance: i32) -> u32,
    /// Debug draw, for `Voxear.Debug` (see [`crate::debug`]): a line or an
    /// arrow, a box, a sphere and text.
    pub debug_line: unsafe extern "C" fn(*const [f32; 3], *const [f32; 3], *const [f32; 4], f32, i32),
    pub debug_box: unsafe extern "C" fn(*const [f32; 3], *const [f32; 3], *const [f32; 4], *const [f32; 4], f32, i32),
    pub debug_sphere: unsafe extern "C" fn(*const [f32; 3], f32, *const [f32; 4], f32, i32),
    pub debug_text: unsafe extern "C" fn(*const [f32; 3], *const u8, i32, *const [f32; 4], f32),
    /// Ends the last sample begun, with the managed bytes allocated while it
    /// ran, as the Profiler's GC Alloc column shows them.
    pub profiler_end_alloc: unsafe extern "C" fn(bytes: u64),
    /// The game's UI, for `Voxear.UiDocument` and `UiElement` (see
    /// [`crate::ui`]): a document loaded, an element found, a document
    /// closed; an element's text, read and set; its classes, visibility,
    /// clicks, value and style.
    pub ui_load: unsafe extern "C" fn(path: *const u8, len: i32) -> i32,
    pub ui_find: unsafe extern "C" fn(document: i32, name: *const u8, len: i32) -> i32,
    pub ui_close: unsafe extern "C" fn(document: i32),
    pub ui_text: unsafe extern "C" fn(element: i32, buffer: *mut u8, capacity: i32) -> i32,
    pub ui_set_text: unsafe extern "C" fn(element: i32, text: *const u8, len: i32),
    pub ui_class: unsafe extern "C" fn(element: i32, class: *const u8, len: i32, change: i32) -> i32,
    pub ui_visible: unsafe extern "C" fn(element: i32, set: i32) -> i32,
    pub ui_clicked: unsafe extern "C" fn(element: i32) -> i32,
    pub ui_value: unsafe extern "C" fn(element: i32, set: i32, value: f32) -> f32,
    pub ui_style: unsafe extern "C" fn(element: i32, style: *const u8, len: i32),
    /// The game's window, for `Screen`: its size, then whether it fills the
    /// screen; and a size and full screen asked for.
    pub screen_get: unsafe extern "C" fn(width: *mut u32, height: *mut u32) -> i32,
    pub screen_set: unsafe extern "C" fn(width: u32, height: u32, fullscreen: i32),
    /// The mouse, for `Cursor`: whether it shows, with how it is held
    /// written to `lock`; and both asked for.
    pub cursor_get: unsafe extern "C" fn(lock: *mut i32) -> i32,
    pub cursor_set: unsafe extern "C" fn(visible: i32, lock: i32),
    /// Single scenes, for `Scene`: one loaded alongside in the background,
    /// one unloaded, and one made the active scene, those two at the end of
    /// the frame.
    pub scene_load_async: unsafe extern "C" fn(reference: *const u8, len: i32, options: *const SceneLoadOptions) -> i32,
    pub scene_unload: unsafe extern "C" fn(reference: *const u8, len: i32),
    pub scene_set_active: unsafe extern "C" fn(reference: *const u8, len: i32),
    /// The game's UI made from code (see [`crate::ui`]): an empty document,
    /// an element made at the end of another, one taken out, and whether
    /// one's value changed.
    pub ui_new: unsafe extern "C" fn() -> i32,
    pub ui_create: unsafe extern "C" fn(parent: i32, kind: i32, text: *const u8, len: i32) -> i32,
    pub ui_remove: unsafe extern "C" fn(element: i32),
    pub ui_changed: unsafe extern "C" fn(element: i32) -> i32,
    /// Saves, for `Voxear.Save` (see [`crate::saves`]): a slot's info and
    /// data queued to be written, returning a ticket; whether a ticket's
    /// write has finished (1) or still runs (0), waiting for it when asked;
    /// a part of a slot read; a slot deleted, copied, renamed or looked for;
    /// and what scripts ask about saves. Failures are below 0, each kind its
    /// own number, with the message `save_info` gives.
    pub save_write: unsafe extern "C" fn(
        slot: *const u8,
        slot_len: i32,
        info: *const u8,
        info_len: i32,
        data: *const u8,
        data_len: i64,
    ) -> i32,
    pub save_wait: unsafe extern "C" fn(ticket: i32, wait: i32) -> i32,
    pub save_read:
        unsafe extern "C" fn(slot: *const u8, slot_len: i32, part: i32, buffer: *mut u8, capacity: i64) -> i64,
    pub save_change:
        unsafe extern "C" fn(change: i32, slot: *const u8, slot_len: i32, other: *const u8, other_len: i32) -> i32,
    pub save_info: unsafe extern "C" fn(what: i32, buffer: *mut u8, capacity: i32) -> i32,
    /// What a save calls something, so a later run finds it again: an Obj
    /// by its scene and its place there (`kind` 0, `id` its entity), an
    /// asset by GUID from its project path or GUID (`kind` 1, `reference`),
    /// or a loaded prefab (`kind` 2) or sound (`kind` 3) by GUID, from its
    /// `id`; written when it fits, its length returned either way, or -1 for
    /// none. Then the Obj a key names, or 0.
    pub save_key:
        unsafe extern "C" fn(kind: i32, id: u64, reference: *const u8, len: i32, buffer: *mut u8, capacity: i32) -> i32,
    pub save_obj: unsafe extern "C" fn(key: *const u8, len: i32) -> u64,
    /// The open scenes' state as JSON, for `SceneState.Capture` (see
    /// [`ScriptServices::scene_state`]), written when it fits, its length
    /// returned either way, or -1 where there are no scenes.
    pub save_scene: unsafe extern "C" fn(buffer: *mut u8, capacity: i32) -> i32,
    /// Sound, for `Sound`, `Audio` and `AudioSource` (see [`crate::audio`]):
    /// a sound loaded by project path or GUID, returning its id, or -1; what
    /// a loaded one holds; one played once, returning its voice, or 0; and a
    /// voice, an Obj's Audio Source or a bus's volume read, and written
    /// when `set` is not 0.
    pub sound_load: unsafe extern "C" fn(reference: *const u8, len: i32) -> i32,
    pub sound_info: unsafe extern "C" fn(sound: i32, out: *mut SoundFacts) -> i32,
    pub sound_play: unsafe extern "C" fn(sound: i32, play: *const PlaySound) -> u64,
    pub voice_value: unsafe extern "C" fn(voice: u64, what: i32, set: i32, value: f32) -> f32,
    pub source_value: unsafe extern "C" fn(entity: u64, what: i32, set: i32, value: f32) -> f32,
    pub audio_bus: unsafe extern "C" fn(bus: i32, set: i32, value: f32) -> f32,
    /// A list view's items, for `ListView` and `TreeView`: how many there
    /// are and how tall each row is, and for a tree, each item's id and its
    /// parent's (0 at the top), `count` pairs in order (see [`crate::ui`]).
    pub ui_items: unsafe extern "C" fn(element: i32, count: i32, parents: *const u64, item_height: f32),
    /// Input, for `Input` and the devices (see [`crate::input`]): an action
    /// found by name, an action this frame, a control by its path, text
    /// about input, and a change to it.
    pub input_action: unsafe extern "C" fn(name: *const u8, len: i32) -> i32,
    pub input_state: unsafe extern "C" fn(action: i32, out: *mut crate::input::InputActionData) -> i32,
    pub input_control: unsafe extern "C" fn(path: *const u8, len: i32, value: *mut [f32; 2]) -> i32,
    pub input_text: unsafe extern "C" fn(what: i32, action: i32, buffer: *mut u8, capacity: i32) -> i32,
    pub input_change:
        unsafe extern "C" fn(what: i32, action: i32, binding: i32, extra: i32, text: *const u8, len: i32) -> i32,
    /// Physics, for `Collider`, `Rigidbody`, `CharacterController` and
    /// `Physics` (see [`crate::physics`]): a value read or written, a
    /// push, a ray, a sphere's overlaps, and a character's move.
    pub physics_value: unsafe extern "C" fn(entity: u64, what: i32, set: i32, value: *mut [f32; 4]) -> i32,
    pub physics_push: unsafe extern "C" fn(entity: u64, kind: i32, amount: *const [f32; 3], at: *const [f32; 3]) -> i32,
    pub physics_raycast: unsafe extern "C" fn(
        origin: *const [f32; 3],
        direction: *const [f32; 3],
        distance: f32,
        mask: u32,
        flags: i32,
        hit: *mut crate::physics::RaycastHitData,
    ) -> i32,
    pub physics_overlap: unsafe extern "C" fn(
        center: *const [f32; 3],
        radius: f32,
        mask: u32,
        flags: i32,
        objs: *mut u64,
        capacity: i32,
    ) -> i32,
    pub character_move: unsafe extern "C" fn(
        entity: u64,
        motion: *const [f32; 3],
        dt: f32,
        state: *mut crate::physics::CharacterMoveData,
    ) -> i32,
    pub joint_connected: unsafe extern "C" fn(entity: u64, set: i32, other: u64) -> u64,
    /// Layers (see [`crate::layers`]): an Obj's, a layer's name, and the
    /// layer a name names.
    pub obj_layer: unsafe extern "C" fn(entity: u64, set: i32, layer: i32) -> i32,
    pub layer_name: unsafe extern "C" fn(layer: i32, buffer: *mut u8, capacity: i32) -> i32,
    pub layer_find: unsafe extern "C" fn(name: *const u8, len: i32) -> i32,
    pub layer_collide: unsafe extern "C" fn(a: i32, b: i32, set: i32, on: i32) -> i32,
    pub text_get: unsafe extern "C" fn(
        key: *const u8,
        len: i32,
        values: *const u8,
        values_len: i32,
        buffer: *mut u8,
        capacity: i32,
    ) -> i32,
    pub text_language: unsafe extern "C" fn(set: i32, code: *const u8, len: i32, buffer: *mut u8, capacity: i32) -> i32,
    pub text_languages: unsafe extern "C" fn(buffer: *mut u8, capacity: i32) -> i32,
    pub text_version: unsafe extern "C" fn() -> u64,
    pub ui_keymap: unsafe extern "C" fn(element: i32, scheme: i32, colors: i32, hidden: i32, map: *const u8, len: i32),
    pub ui_keymap_clicked: unsafe extern "C" fn(element: i32, buffer: *mut u8, capacity: i32) -> i32,
    pub input_uses: unsafe extern "C" fn(path: *const u8, len: i32, buffer: *mut u8, capacity: i32) -> i32,
    pub input_label: unsafe extern "C" fn(path: *const u8, len: i32, buffer: *mut u8, capacity: i32) -> i32,
    pub input_conflicts:
        unsafe extern "C" fn(action: i32, path: *const u8, len: i32, buffer: *mut u8, capacity: i32) -> i32,
    pub text_asset: unsafe extern "C" fn(key: *const u8, len: i32, buffer: *mut u8, capacity: i32) -> i32,
    pub ui_set_string: unsafe extern "C" fn(element: i32, which: i32, text: *const u8, len: i32),
    /// `Lighting.Apply`: lights the level with lighting settings, by GUID,
    /// project path or built-in name.
    pub lighting_apply: unsafe extern "C" fn(reference: *const u8, len: i32),
    /// `Seasons.TimeOfYear`: the level's, NaN without Seasons, and setting it.
    pub seasons_time: unsafe extern "C" fn() -> f32,
    pub seasons_set_time: unsafe extern "C" fn(time: f32),
    /// `Water`: the water at a point, its surface's height, the way it faces
    /// and its flow, seven numbers; 0 where there is none.
    pub water_sample: unsafe extern "C" fn(at: *const [f32; 3], out: *mut [f32; 7]) -> i32,
    /// `Water.Features`: which of the sea's costlier looks are on, as bits,
    /// -1 without an Ocean, and setting them.
    pub water_features: unsafe extern "C" fn() -> i32,
    pub water_set_features: unsafe extern "C" fn(features: i32),
    /// `Light`: reads or writes a value of an Obj's Light, by number (as
    /// `NativeLights` in Voxear.Core), up to four floats; 1 when it did.
    pub light_value: unsafe extern "C" fn(entity: u64, what: i32, set: i32, value: *mut [f32; 4]) -> i32,
    /// `Draw.ScreenText`: a line of text in the view's corner, its colour
    /// and how long it shows.
    pub debug_screen_text: unsafe extern "C" fn(text: *const u8, len: i32, color: *const [f32; 4], duration: f32),
    /// `Draw`'s solid shapes: triangles, three corners each, as `count`
    /// says, in a colour, for a while, drawn as `flags` say.
    pub debug_solid: unsafe extern "C" fn(corners: *const [f32; 3], count: i32, color: *const [f32; 4], duration: f32, flags: i32),
    /// Reads, or with `set` writes, value `what` of an Obj's Particle
    /// System, as `light_value` does an Obj's Light.
    pub particle_value: unsafe extern "C" fn(entity: u64, what: i32, set: i32, value: *mut [f32; 4]) -> i32,
    /// Smoke: a shot from one point to another (0), a blast at a point with
    /// a radius (1), or how much shows from one point to another through it
    /// (2), which it gives back.
    pub smoke_act: unsafe extern "C" fn(what: i32, values: *const [f32; 7]) -> f32,
    /// Reads, or with `set` writes, value `what` of an Obj's Smoke, as
    /// `light_value` does an Obj's Light.
    pub smoke_value: unsafe extern "C" fn(entity: u64, what: i32, set: i32, value: *mut [f32; 4]) -> i32,
    /// `Obj.Visible`: whether an Obj is drawn, set first when `set`; -1 for
    /// one that is gone.
    pub obj_visible: unsafe extern "C" fn(entity: u64, set: i32, visible: i32) -> i32,
    /// Reads, or with `set` writes, value `what` of an Obj's Mist, as
    /// `light_value` does an Obj's Light.
    pub mist_value: unsafe extern "C" fn(entity: u64, what: i32, set: i32, value: *mut [f32; 4]) -> i32,
    /// Reads, or with `set` writes, value `what` of an Obj's Motion
    /// Matcher, as `light_value` does an Obj's Light.
    pub motion_matcher_value: unsafe extern "C" fn(entity: u64, what: i32, set: i32, value: *mut [f32; 4]) -> i32,
    /// Reads, or with `set` writes, value `what` of an Obj's Bone
    /// Attachment, as `light_value` does an Obj's Light.
    pub bone_attachment_value: unsafe extern "C" fn(entity: u64, what: i32, set: i32, value: *mut [f32; 4]) -> i32,
    /// `Net`: networked play, by number (as `NativeNet` in Voxear.Core),
    /// with two whole numbers, some text and four floats for what it gives
    /// back besides its answer.
    pub net_call: unsafe extern "C" fn(what: i32, a: i32, b: i32, text: *const u8, len: i32, out: *mut [f32; 4]) -> i32,
    /// Reads text `what` of an Obj's Motion Matcher, or does what it asks
    /// with some text, as playing an action by its name: the answer's
    /// length, written when it fits, or -1.
    pub motion_matcher_text:
        unsafe extern "C" fn(entity: u64, what: i32, text: *const u8, len: i32, buffer: *mut u8, capacity: i32) -> i32,
    /// Reads, or with `set` writes, value `what` of an Obj's Net Object, as
    /// `light_value` does an Obj's Light.
    pub net_object_value: unsafe extern "C" fn(entity: u64, what: i32, set: i32, value: *mut [f32; 4]) -> i32,
    /// `[Replicated]` fields: a behaviour's, on a server, handed over by
    /// its type's key as bytes; with `take`, the next that came from the
    /// server, into `entity`, `key` and up to `capacity` bytes, its length
    /// given back, or -1 for none.
    pub net_fields: unsafe extern "C" fn(take: i32, entity: *mut u64, key: *mut u32, bytes: *mut u8, len: i32) -> i32,
    /// RPCs: with `what` 0 a behaviour's method to run on the server, 1 on
    /// players' machines (`player` one of them, or -1 for all), 2 and 3 the
    /// same but allowed to be lost, by `entity`, `key` and `len` bytes of
    /// arguments, giving back 1 if it went. With 4, the next that came, into
    /// `entity`, `key`, `player` (who made it, -1 for the server) and up to
    /// `len` bytes: its length, or -1 for none.
    pub net_rpc: unsafe extern "C" fn(what: i32, entity: *mut u64, key: *mut u32, player: *mut i32, bytes: *mut u8, len: i32) -> i32,
    /// A ray as `player` saw the world when they made their input for
    /// `tick`, as `physics_raycast` casts one now.
    pub net_raycast: unsafe extern "C" fn(
        player: i32,
        tick: i32,
        origin: *const [f32; 3],
        direction: *const [f32; 3],
        distance: f32,
        mask: u32,
        flags: i32,
        hit: *mut crate::physics::RaycastHitData,
    ) -> i32,
    /// Reads or writes a value of an Obj's component of a kind (see
    /// [`ComponentKind`]), as `smoke_value` does a Smoke's: 1 when the Obj
    /// has one.
    pub kind_value: unsafe extern "C" fn(kind: i32, entity: u64, what: i32, set: i32, value: *mut [f32; 4]) -> i32,
    /// Reads text `what` of an Obj's component of a kind, or does what it
    /// asks with some text, as `motion_matcher_text` does a Motion
    /// Matcher's: the answer's length, written when it fits, or -1.
    pub kind_text: unsafe extern "C" fn(
        kind: i32,
        entity: u64,
        what: i32,
        text: *const u8,
        len: i32,
        buffer: *mut u8,
        capacity: i32,
    ) -> i32,
    /// Changes a posed model's bones, or reads them, by `op`, as the
    /// engine's `animation::pose_ops` numbers them: bones and sides in
    /// `ints`, places, ways and weights in `floats`, a clip's reference in
    /// `text`. What it gives back goes in `out`: how many it wrote, or -1
    /// when the Obj draws no model with bones or it cannot be done.
    pub pose_op: unsafe extern "C" fn(
        entity: u64,
        op: i32,
        ints: *const i32,
        ints_len: i32,
        floats: *const f32,
        floats_len: i32,
        text: *const u8,
        text_len: i32,
        out: *mut f32,
        out_len: i32,
    ) -> i32,
    /// A data asset by project path or GUID, as JSON for
    /// `Definition.Load`: its length, written when it fits, or -1.
    pub asset_fields: unsafe extern "C" fn(reference: *const u8, len: i32, buffer: *mut u8, capacity: i32) -> i32,
    /// `Terrain`: how many terrains the level has.
    pub terrain_count: unsafe extern "C" fn() -> i32,
    /// `Terrain`: the ground under a point, its height and the way it faces,
    /// and the terrain's Obj; 0 where no terrain reaches.
    pub terrain_ground: unsafe extern "C" fn(at: *const [f32; 3], out: *mut [f32; 4], obj: *mut u64) -> i32,
    /// `Terrain`: the first place a ray meets any terrain within a distance.
    pub terrain_raycast: unsafe extern "C" fn(
        origin: *const [f32; 3],
        direction: *const [f32; 3],
        distance: f32,
        hit: *mut crate::physics::RaycastHitData,
    ) -> i32,
    /// `Obj.Active` and a component's `Enabled` (see `switches`).
    pub obj_switch: unsafe extern "C" fn(entity: u64, what: i32, set: i32) -> i32,
    /// Copies an Obj and every Obj under it, as `prefab_instantiate` makes
    /// a prefab's, with their behaviours' handles for `batch_handles`.
    pub obj_instantiate: unsafe extern "C" fn(
        original: u64,
        placement: *const Transform,
        world_space: i32,
        parent: u64,
        out_root: *mut u64,
    ) -> i32,
    /// `Plant`: whether one of a Foliage's plants stands, where and how
    /// large (see `plants`).
    pub plant_info: unsafe extern "C" fn(obj: u64, number: u32, at: *mut [f32; 3], size: *mut f32) -> i32,
    /// `Plant.Remove`: takes a plant away.
    pub plant_remove: unsafe extern "C" fn(obj: u64, number: u32) -> i32,
    /// `Plant.CutDown`: takes a plant away and makes its Cut prefab there.
    pub plant_cut: unsafe extern "C" fn(obj: u64, number: u32, out_root: *mut u64) -> i32,
    /// `Plants.Query`: the plants standing near a point, nearest first.
    pub plants_near: unsafe extern "C" fn(
        centre: *const [f32; 3],
        radius: f32,
        out: *mut crate::plants::PlantRef,
        capacity: i32,
    ) -> i32,
    /// `Terrain.Apply`: changes the ground of every terrain an edit
    /// reaches, with the surface it lays by its material (see `terrain`).
    pub terrain_edit:
        unsafe extern "C" fn(edit: *const crate::terrain::TerrainEditData, surface: *const u8, surface_len: i32) -> i32,
    /// `Terrain.CaptureEdits` and `Terrain.ApplyEdits`: every edited
    /// terrain's edits as bytes, and back.
    pub terrain_edits: unsafe extern "C" fn(what: i32, data: *const u8, len: i32, out: *mut u8, capacity: i32) -> i32,
    /// `element.Style`: one property of an element's inline style.
    pub ui_style_property:
        unsafe extern "C" fn(element: i32, name: *const u8, name_len: i32, value: *const u8, value_len: i32),
    /// Every data asset of the project, as JSON for `Definition.All`: its
    /// length, written when it fits, or -1.
    pub data_assets: unsafe extern "C" fn(buffer: *mut u8, capacity: i32) -> i32,
    /// `Blocks.At`: the block at a point in the first world of blocks with
    /// one there, and its Obj; 0 for air (see `blocks`).
    pub block_at: unsafe extern "C" fn(at: *const [f32; 3], obj: *mut u64) -> i32,
    /// `Blocks.Set`, `Fill` and `Ball`: sets the blocks a shape reaches in
    /// every world of blocks; how many changed.
    pub block_set:
        unsafe extern "C" fn(shape: i32, a: *const [f32; 3], b: *const [f32; 3], block: i32, only: i32) -> i32,
    /// `Blocks.Raycast`: the first block a ray meets within a distance,
    /// liquids passed through unless the flags say to meet them.
    pub block_raycast: unsafe extern "C" fn(
        origin: *const [f32; 3],
        direction: *const [f32; 3],
        distance: f32,
        flags: i32,
        hit: *mut crate::blocks::BlockHitData,
    ) -> i32,
    /// `Blocks.CaptureEdits` and `Blocks.ApplyEdits`: every changed world of
    /// blocks' changes as bytes, and back.
    pub block_edits: unsafe extern "C" fn(what: i32, data: *const u8, len: i32, out: *mut u8, capacity: i32) -> i32,
    /// `Physics.GravityAt`: which way and how hard things fall at a point.
    pub physics_gravity_at: unsafe extern "C" fn(at: *const [f32; 3], gravity: *mut [f32; 3]) -> i32,
    /// Where the world's middle is from the level's, in whole metres, as x,
    /// y and z (see `voxear_scene::WorldOrigin`): zero until it first moves.
    pub world_origin: unsafe extern "C" fn(out: *mut i64),
    /// `Block`: what a `.block` says, by project path or GUID, as JSON; its
    /// length, written when it fits, or -1.
    pub block_info: unsafe extern "C" fn(reference: *const u8, len: i32, buffer: *mut u8, capacity: i32) -> i32,
    /// `BlockHit.Definition`: the GUID of the block numbered so in the
    /// world of blocks on an Obj; its length, or 0 for none.
    pub block_of: unsafe extern "C" fn(obj: u64, block: i32, buffer: *mut u8, capacity: i32) -> i32,
    /// `Blocks.Set`, `Fill`, `Ball` and `Place` with a `Block`: as
    /// `block_set`, the block numbered in each world as that world has it.
    pub block_set_named: unsafe extern "C" fn(
        shape: i32,
        a: *const [f32; 3],
        b: *const [f32; 3],
        block: *const u8,
        len: i32,
        only: i32,
    ) -> i32,
    /// Goes up whenever a block is saved, so `Block` reads it again.
    pub blocks_version: unsafe extern "C" fn() -> u64,
    /// `Water.Splash`: throws up a splash where the water is over or under
    /// a point, so wide; 0 where there is no water.
    pub water_splash: unsafe extern "C" fn(at: *const [f32; 3], size: f32) -> i32,
    /// The cave rooms of a world of blocks, as JSON for `Caves`.
    pub caves_info:
        unsafe extern "C" fn(obj: u64, what: i32, at: *const [f32; 4], buffer: *mut u8, capacity: i32) -> i32,
    /// Sets off a blast at a point, so wide, that throws the particles round
    /// it out, those at its middle so fast.
    pub particle_blast: unsafe extern "C" fn(at: *const [f32; 3], radius: f32, speed: f32) -> i32,
    /// `Blocks.All`: every block of the project, by GUID, a line each; the
    /// text's length, written when it fits.
    pub blocks_all: unsafe extern "C" fn(buffer: *mut u8, capacity: i32) -> i32,
    /// `World.Quality` and the numbers under it: what making worlds of
    /// blocks may spend, read by number.
    pub world_budget: unsafe extern "C" fn(which: i32) -> f32,
    /// Sets one of them, taken up at once.
    pub set_world_budget: unsafe extern "C" fn(which: i32, value: f32),
    /// Reads, or with `set` writes, value `what` of an Obj's Mob Motion,
    /// as `light_value` does an Obj's Light.
    pub mob_motion_value: unsafe extern "C" fn(entity: u64, what: i32, set: i32, value: *mut [f32; 4]) -> i32,
    /// Reads, or with `set` writes, value `what` of an Obj's Block Mob, as
    /// `light_value` does an Obj's Light.
    pub block_mob_value: unsafe extern "C" fn(entity: u64, what: i32, set: i32, value: *mut [f32; 4]) -> i32,
    /// `Biome`: what a `.biome` says, by project path or GUID, as JSON; its
    /// length, written when it fits, or -1.
    pub biome_info: unsafe extern "C" fn(reference: *const u8, len: i32, buffer: *mut u8, capacity: i32) -> i32,
    /// `Biomes.At`: the GUID of the biome a place is; its length, or 0 where
    /// no world of blocks with biomes reaches.
    pub biome_at: unsafe extern "C" fn(at: *const [f32; 3], buffer: *mut u8, capacity: i32) -> i32,
    /// `Weather`: the kind the level's weather is or is turning to, with eleven numbers written to `state`: what
    /// it adds this moment (cloud, darkness, fog, wind, rain, snow, lightning, dust), how wet the ground is, how
    /// much snow has settled, and how bright its lightning is; and whether it is still turning to `turning`. And
    /// turning it over `seconds` to a kind, or with `amounts`, the first eight of those, to them.
    pub weather_state: unsafe extern "C" fn(state: *mut [f32; 11], turning: *mut i32) -> i32,
    pub weather_set: unsafe extern "C" fn(kind: i32, amounts: *const [f32; 8], seconds: f32),
    /// `LookPacks.Use`: wears the look pack a reference names from the next frame, or with none, takes off the
    /// one worn; and, with `remember`, keeps the choice beside the saves for the game's next run.
    pub look_pack_wear: unsafe extern "C" fn(reference: *const u8, len: i32, remember: i32),
    /// `LookPacks`: every look pack there is to wear, which is worn and where a player keeps their own, as JSON;
    /// its length, or -1 when there is none.
    pub look_packs_info: unsafe extern "C" fn(buffer: *mut u8, capacity: i32) -> i32,
    /// `Heat.Sample` and `Heat.At`: the temperature at a point, in °C, and
    /// what makes it so into `out`: the climate's, what the time of day
    /// adds, how sheltered it is, what shelter adds, what zones add, what
    /// hot and cold things near add, and how wet it is (see `heat`).
    pub heat_sample: unsafe extern "C" fn(at: *const [f32; 3], out: *mut [f32; 8]) -> f32,
    /// `Heat.AddSource`: heat at a point, so hot and so wide, for so long.
    pub heat_add: unsafe extern "C" fn(at: *const [f32; 3], celsius: f32, radius: f32, seconds: f32) -> i32,
    /// `Clock`: the level's time of day (0), day length in minutes (1) and
    /// whether it turns the sun (2), set first when `set` is not 0.
    pub clock_value: unsafe extern "C" fn(what: i32, set: i32, value: f32) -> f32,
    /// `Storms`, `Storm` and `Sea`: what `what` asks of the storm, whirlpool or setting numbered `id`, with sixteen
    /// numbers given and written (see `voxear_engine::storms::scripts`).
    pub storm_call: unsafe extern "C" fn(what: i32, id: u32, values: *mut [f32; 16]) -> i32,
    /// `Storms.CaptureState` and `Storms.ApplyState`: the storms' clock and those scripts made, as bytes, and back.
    pub storm_state: unsafe extern "C" fn(what: i32, data: *const u8, len: i32, out: *mut u8, capacity: i32) -> i32,
    /// `TexturePacks.Use`: wears the texture pack a reference names from the next frame, or with none, takes off
    /// the one worn; and, with `remember`, keeps the choice beside the saves for the game's next run.
    pub texture_pack_wear: unsafe extern "C" fn(reference: *const u8, len: i32, remember: i32),
    /// `TexturePacks`: every texture pack there is to wear, which is worn and where a player keeps their own, as
    /// JSON; its length, or -1 when there is none.
    pub texture_packs_info: unsafe extern "C" fn(buffer: *mut u8, capacity: i32) -> i32,
    /// `TexturePacks.WriteStarter`: makes a pack to start from, called `name`, in the player's folder, and writes
    /// what names it; its length, or -1 where it could not be made.
    pub texture_pack_starter: unsafe extern "C" fn(name: *const u8, len: i32, buffer: *mut u8, capacity: i32) -> i32,
}

/// The kinds of component scripts reach through one entry, `kind_value`,
/// each answered by whatever set it with [`set_kind_values`]: a module's
/// components among them, which answer nothing in a game without it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum ComponentKind {
    Camera = 1,
    FirstPersonView = 2,
    LimbIk = 3,
    UiPanel = 4,
    /// A model's bones, as its Skeleton.
    Skeleton = 5,
    /// The mesh an Obj draws, for its box.
    MeshRenderer = 6,
    /// Where an Obj is in the hierarchy: its parent and its children.
    Hierarchy = 7,
    /// A document shown over the game's view.
    ScreenUi = 8,
    /// What moves the particles round an Obj.
    ForceField = 9,
    /// A Particle System's effect, and effects played at a place.
    Particles = 10,
    Gun = 16,
    GunHolder = 17,
    /// A minimap and a map of the world on the screen, and what every map
    /// follows, is titled and has placed on it.
    Map = 20,
    /// An Obj's mark on every map.
    MapMarker = 21,
    /// What takes the temperature of where it is, and changes with it.
    HeatBody = 22,
    /// Something hot or cold, warming or cooling what is near it.
    HeatSource = 23,
    /// An area hotter or colder than round it.
    HeatZone = 24,
}

/// The most kinds `kind_value` answers.
const KINDS: usize = 32;
static KIND_VALUES: [std::sync::OnceLock<ComponentValues>; KINDS] = [const { std::sync::OnceLock::new() }; KINDS];

/// Sets what answers a kind of component in scripts; the first set is kept.
pub fn set_kind_values(kind: ComponentKind, values: ComponentValues) {
    let _ = KIND_VALUES[kind as usize].set(values);
}

unsafe extern "C" fn kind_value(kind: i32, entity: u64, what: i32, set: i32, value: *mut [f32; 4]) -> i32 {
    let values = usize::try_from(kind).ok().and_then(|kind| KIND_VALUES.get(kind)).and_then(|cell| cell.get());
    unsafe { component_value(values, entity, what, set, value) }
}

/// What answers a kind of component's calls with text in scripts: `None`
/// for no component, or nothing to give back.
pub type ComponentTexts = fn(&mut World, Entity, i32, &str) -> Option<String>;
static KIND_TEXTS: [std::sync::OnceLock<ComponentTexts>; KINDS] = [const { std::sync::OnceLock::new() }; KINDS];

/// Sets what answers a kind of component's calls with text; the first set
/// is kept.
pub fn set_kind_texts(kind: ComponentKind, texts: ComponentTexts) {
    let _ = KIND_TEXTS[kind as usize].set(texts);
}

unsafe extern "C" fn kind_text(
    kind: i32,
    entity: u64,
    what: i32,
    text: *const u8,
    len: i32,
    buffer: *mut u8,
    capacity: i32,
) -> i32 {
    let texts = usize::try_from(kind).ok().and_then(|kind| KIND_TEXTS.get(kind)).and_then(|cell| cell.get());
    let Some(texts) = texts else { return -1 };
    let text = unsafe { utf8(text, len) };
    let Some(answer) = with_world(|world| texts(world, Entity::from_bits(entity), what, &text)) else { return -1 };
    if !buffer.is_null() && answer.len() <= capacity.max(0) as usize {
        unsafe { ptr::copy_nonoverlapping(answer.as_ptr(), buffer, answer.len()) };
    }
    answer.len() as i32
}

/// What answers scripts' posing of a model's bones (see `pose_op`): the
/// engine's, set when it starts. It writes into its last slice and says how
/// many it wrote, or `None` when the op cannot be done.
pub type PoseOps = fn(&mut World, Entity, i32, &[i32], &[f32], &str, &mut [f32]) -> Option<usize>;
static POSE_OPS: std::sync::OnceLock<PoseOps> = std::sync::OnceLock::new();

/// Sets what answers `pose_op`; the first set is kept.
pub fn set_pose_ops(ops: PoseOps) {
    let _ = POSE_OPS.set(ops);
}

#[allow(clippy::too_many_arguments)]
unsafe extern "C" fn pose_op(
    entity: u64,
    op: i32,
    ints: *const i32,
    ints_len: i32,
    floats: *const f32,
    floats_len: i32,
    text: *const u8,
    text_len: i32,
    out: *mut f32,
    out_len: i32,
) -> i32 {
    let Some(ops) = POSE_OPS.get() else { return -1 };
    // C# hands its arrays over as they are, which may not be aligned for Rust's slices: copied, read unaligned.
    let read = |ptr: *const u8, len: i32, size: usize| -> Vec<u8> {
        if ptr.is_null() || len <= 0 {
            return Vec::new();
        }
        unsafe { std::slice::from_raw_parts(ptr, len as usize * size) }.to_vec()
    };
    let word = |bytes: &[u8]| -> [u8; 4] { bytes.try_into().expect("four bytes") };
    let ints: Vec<i32> = read(ints.cast(), ints_len, 4).chunks_exact(4).map(|b| i32::from_le_bytes(word(b))).collect();
    let floats: Vec<f32> = read(floats.cast(), floats_len, 4).chunks_exact(4).map(|b| f32::from_le_bytes(word(b))).collect();
    let text = unsafe { utf8(text, text_len) };
    let mut answer = vec![0f32; out_len.max(0) as usize];
    let written = with_world(|world| ops(world, Entity::from_bits(entity), op, &ints, &floats, &text, &mut answer));
    let Some(written) = written else { return -1 };
    let written = written.min(answer.len());
    if !out.is_null() {
        for (index, value) in answer[..written].iter().enumerate() {
            unsafe { out.add(index).write_unaligned(*value) };
        }
    }
    written as i32
}

/// What answers scripts' questions about water: the engine's, set when it
/// starts (voxear_scripting cannot see the renderer's sea itself).
pub type WaterSampler = fn(&World, [f32; 3]) -> Option<[f32; 7]>;
static WATER: std::sync::OnceLock<WaterSampler> = std::sync::OnceLock::new();

/// Sets what answers `Water` in scripts; the first set is kept.
pub fn set_water_sampler(sampler: WaterSampler) {
    let _ = WATER.set(sampler);
}

/// What throws up a splash for scripts where the water is over or under a
/// point, so wide: the engine's, set when it starts. Whether there was water
/// there.
pub type WaterSplasher = fn(&mut World, [f32; 3], f32) -> bool;
static SPLASHER: std::sync::OnceLock<WaterSplasher> = std::sync::OnceLock::new();

/// Sets what `Water.Splash` in scripts calls; the first set is kept.
pub fn set_water_splasher(splasher: WaterSplasher) {
    let _ = SPLASHER.set(splasher);
}

/// What sets off a blast for scripts at a point, so wide, throwing
/// particles out so fast: the engine's, set when it starts.
pub type ParticleBlaster = fn(&mut World, [f32; 3], f32, f32);
static BLASTER: std::sync::OnceLock<ParticleBlaster> = std::sync::OnceLock::new();

/// Sets what `Particles.Push` in scripts calls; the first set is kept.
pub fn set_particle_blaster(blaster: ParticleBlaster) {
    let _ = BLASTER.set(blaster);
}

/// What reads, or with a value writes, a value of one of an Obj's
/// components for scripts, by number: the engine's, set when it starts
/// (voxear_scripting cannot see the renderer's components). `None` when the
/// Obj has none.
pub type ComponentValues = fn(&mut World, Entity, i32, Option<[f32; 4]>) -> Option<[f32; 4]>;
/// An Obj's Light's values (see `ComponentValues`).
pub type LightValues = ComponentValues;
static LIGHTS: std::sync::OnceLock<ComponentValues> = std::sync::OnceLock::new();
static PARTICLES: std::sync::OnceLock<ComponentValues> = std::sync::OnceLock::new();
static SMOKES: std::sync::OnceLock<ComponentValues> = std::sync::OnceLock::new();
static MISTS: std::sync::OnceLock<ComponentValues> = std::sync::OnceLock::new();
static MATCHERS: std::sync::OnceLock<ComponentValues> = std::sync::OnceLock::new();
static ATTACHMENTS: std::sync::OnceLock<ComponentValues> = std::sync::OnceLock::new();
static NET_OBJECTS: std::sync::OnceLock<ComponentValues> = std::sync::OnceLock::new();

/// What hands `[Replicated]` fields between scripts and the engine: given a
/// behaviour's on a server, or with `None`, the next from the server.
pub type NetFields = fn(&mut World, Option<(Entity, u32, Vec<u8>)>) -> Option<(Entity, u32, Vec<u8>)>;
static NET_FIELDS: std::sync::OnceLock<NetFields> = std::sync::OnceLock::new();

/// Sets what hands `[Replicated]` fields over; the first set is kept.
pub fn set_net_fields(fields: NetFields) {
    let _ = NET_FIELDS.set(fields);
}

unsafe extern "C" fn net_fields(take: i32, entity: *mut u64, key: *mut u32, bytes: *mut u8, len: i32) -> i32 {
    let Some(fields) = NET_FIELDS.get() else { return -1 };
    if entity.is_null() || key.is_null() || bytes.is_null() || len < 0 {
        return -1;
    }
    if take == 0 {
        let given = unsafe { std::slice::from_raw_parts(bytes, len as usize) }.to_vec();
        let (entity, key) = unsafe { (Entity::from_bits(*entity), *key) };
        with_world(|world| fields(world, Some((entity, key, given))));
        return 0;
    }
    let Some((from, at, value)) = with_world(|world| fields(world, None)) else { return -1 };
    let copied = value.len().min(len as usize);
    unsafe {
        *entity = from.to_bits();
        *key = at;
        std::ptr::copy_nonoverlapping(value.as_ptr(), bytes, copied);
    }
    copied as i32
}

/// An RPC a script makes: a behaviour's method on an Obj, by a key, with
/// its arguments as bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RpcSend {
    pub entity: Entity,
    pub key: u32,
    /// To players' machines rather than the server.
    pub to_players: bool,
    /// One player, or every player for `None`.
    pub player: Option<u16>,
    pub reliable: bool,
    pub bytes: Vec<u8>,
}

/// An RPC that came for a script to run: `caller` is the player who made
/// it, `None` for the server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RpcTaken {
    pub entity: Entity,
    pub key: u32,
    pub caller: Option<u16>,
    pub bytes: Vec<u8>,
}

/// What sends scripts' RPCs, and hands them those that came.
#[derive(Clone, Copy)]
pub struct NetRpcs {
    pub send: fn(&mut World, RpcSend) -> bool,
    pub take: fn(&mut World) -> Option<RpcTaken>,
}
static NET_RPCS: std::sync::OnceLock<NetRpcs> = std::sync::OnceLock::new();

/// Sets what sends RPCs; the first set is kept.
pub fn set_net_rpcs(rpcs: NetRpcs) {
    let _ = NET_RPCS.set(rpcs);
}

unsafe extern "C" fn net_rpc(what: i32, entity: *mut u64, key: *mut u32, player: *mut i32, bytes: *mut u8, len: i32) -> i32 {
    let Some(rpcs) = NET_RPCS.get().copied() else { return -1 };
    if entity.is_null() || key.is_null() || player.is_null() || len < 0 || (bytes.is_null() && len > 0) {
        return -1;
    }
    if (0..4).contains(&what) {
        let given = if len == 0 { Vec::new() } else { unsafe { std::slice::from_raw_parts(bytes, len as usize) }.to_vec() };
        let (entity, key, player) = unsafe { (Entity::from_bits(*entity), *key, *player) };
        let send = RpcSend {
            entity,
            key,
            to_players: what & 1 == 1,
            player: u16::try_from(player).ok(),
            reliable: what < 2,
            bytes: given,
        };
        return i32::from(with_world(|world| Some((rpcs.send)(world, send))).unwrap_or(false));
    }
    let Some(taken) = with_world(|world| (rpcs.take)(world)) else { return -1 };
    let copied = taken.bytes.len().min(len as usize);
    unsafe {
        *entity = taken.entity.to_bits();
        *key = taken.key;
        *player = taken.caller.map_or(-1, i32::from);
        if copied > 0 {
            std::ptr::copy_nonoverlapping(taken.bytes.as_ptr(), bytes, copied);
        }
    }
    copied as i32
}

/// What casts a ray as a player saw the world: the player, the tick of
/// their input, the ray and its reach, the layers, and whether triggers
/// count.
pub type NetRaycast = fn(&mut World, u16, u64, voxear_core::math::Vec3, voxear_core::math::Vec3, f32, u32, bool) -> Option<voxear_physics::scene::SceneHit>;
static NET_RAYCAST: std::sync::OnceLock<NetRaycast> = std::sync::OnceLock::new();

/// Sets what casts rays as players saw the world; the first set is kept.
pub fn set_net_raycast(raycast: NetRaycast) {
    let _ = NET_RAYCAST.set(raycast);
}

#[allow(clippy::too_many_arguments)]
unsafe extern "C" fn net_raycast(
    player: i32,
    tick: i32,
    origin: *const [f32; 3],
    direction: *const [f32; 3],
    distance: f32,
    mask: u32,
    flags: i32,
    hit: *mut crate::physics::RaycastHitData,
) -> i32 {
    let Some(raycast) = NET_RAYCAST.get().copied() else { return 0 };
    let (Ok(player), Ok(tick)) = (u16::try_from(player), u64::try_from(tick)) else { return 0 };
    if origin.is_null() || direction.is_null() || hit.is_null() {
        return 0;
    }
    let (origin, direction) = unsafe { (*origin, *direction) };
    let (origin, direction) = (voxear_core::math::Vec3::from_array(origin), voxear_core::math::Vec3::from_array(direction));
    let triggers = flags & 1 != 0;
    let Some(found) = with_world(|world| raycast(world, player, tick, origin, direction, distance, mask, triggers)) else {
        return 0;
    };
    unsafe {
        *hit = crate::physics::RaycastHitData {
            obj: found.obj.to_bits(),
            point: found.point.to_array(),
            normal: found.normal.to_array(),
            distance: found.distance,
            plant: found.plant.map_or(0, |number| number + 1),
        };
    }
    1
}

/// Sets what answers `NetObject` in scripts; the first set is kept.
pub fn set_net_object_values(values: ComponentValues) {
    let _ = NET_OBJECTS.set(values);
}

unsafe extern "C" fn net_object_value(entity: u64, what: i32, set: i32, value: *mut [f32; 4]) -> i32 {
    unsafe { component_value(NET_OBJECTS.get(), entity, what, set, value) }
}

/// Sets what answers `Light` in scripts; the first set is kept.
pub fn set_light_values(values: LightValues) {
    let _ = LIGHTS.set(values);
}

/// Sets what answers `ParticleSystem` in scripts; the first set is kept.
pub fn set_particle_values(values: ComponentValues) {
    let _ = PARTICLES.set(values);
}

/// Sets what answers `Smoke` in scripts; the first set is kept.
pub fn set_smoke_values(values: ComponentValues) {
    let _ = SMOKES.set(values);
}

/// Sets what answers `Mist` in scripts; the first set is kept.
pub fn set_mist_values(values: ComponentValues) {
    let _ = MISTS.set(values);
}

/// Sets what answers `MotionMatcher` in scripts; the first set is kept.
pub fn set_motion_matcher_values(values: ComponentValues) {
    let _ = MATCHERS.set(values);
}

/// What answers `MotionMatcher`'s calls with text in scripts: the engine's,
/// set when it starts. `None` for no matcher, or nothing to give back.
pub type MotionMatcherTexts = fn(&mut World, Entity, i32, &str) -> Option<String>;
static MATCHER_TEXTS: std::sync::OnceLock<MotionMatcherTexts> = std::sync::OnceLock::new();

/// Sets what answers `MotionMatcher`'s calls with text; the first set is kept.
pub fn set_motion_matcher_texts(texts: MotionMatcherTexts) {
    let _ = MATCHER_TEXTS.set(texts);
}

unsafe extern "C" fn motion_matcher_text(
    entity: u64,
    what: i32,
    text: *const u8,
    len: i32,
    buffer: *mut u8,
    capacity: i32,
) -> i32 {
    let Some(texts) = MATCHER_TEXTS.get() else { return -1 };
    let text = unsafe { utf8(text, len) };
    let Some(answer) = with_world(|world| texts(world, Entity::from_bits(entity), what, &text)) else { return -1 };
    if !buffer.is_null() && answer.len() <= capacity.max(0) as usize {
        unsafe { ptr::copy_nonoverlapping(answer.as_ptr(), buffer, answer.len()) };
    }
    answer.len() as i32
}

/// Sets what answers `BoneAttachment` in scripts; the first set is kept.
pub fn set_bone_attachment_values(values: ComponentValues) {
    let _ = ATTACHMENTS.set(values);
}

/// What answers `Net` in scripts: the engine's, set when it starts. Given
/// what to do, two numbers, some text and room for floats to give back.
pub type NetCalls = fn(&mut World, i32, i32, i32, &str, &mut [f32; 4]) -> i32;
static NET: std::sync::OnceLock<NetCalls> = std::sync::OnceLock::new();

/// Sets what answers `Net` in scripts; the first set is kept.
pub fn set_net_calls(calls: NetCalls) {
    let _ = NET.set(calls);
}

unsafe extern "C" fn net_call(what: i32, a: i32, b: i32, text: *const u8, len: i32, out: *mut [f32; 4]) -> i32 {
    let Some(calls) = NET.get() else { return 0 };
    let text = unsafe { utf8(text, len) };
    let mut values = [0.0f32; 4];
    let answer = with_world(|world| Some(calls(world, what, a, b, &text, &mut values))).unwrap_or(0);
    if !out.is_null() {
        unsafe { *out = values };
    }
    answer
}

/// What answers scripts' shots and blasts through smoke and their asking
/// through it: the engine's, set when it starts. Given what to do and its
/// numbers, as `NativeApi::smoke_act` says.
pub type SmokeActions = fn(&mut World, i32, [f32; 7]) -> f32;
static SMOKE: std::sync::OnceLock<SmokeActions> = std::sync::OnceLock::new();

/// Sets what answers `Smokes` in scripts; the first set is kept.
pub fn set_smoke_actions(actions: SmokeActions) {
    let _ = SMOKE.set(actions);
}

unsafe extern "C" fn smoke_act(what: i32, values: *const [f32; 7]) -> f32 {
    let (Some(actions), false) = (SMOKE.get(), values.is_null()) else { return 1.0 };
    let values = unsafe { *values };
    with_world(|world| Some(actions(world, what, values))).unwrap_or(1.0)
}

unsafe extern "C" fn light_value(entity: u64, what: i32, set: i32, value: *mut [f32; 4]) -> i32 {
    unsafe { component_value(LIGHTS.get(), entity, what, set, value) }
}

unsafe extern "C" fn particle_value(entity: u64, what: i32, set: i32, value: *mut [f32; 4]) -> i32 {
    unsafe { component_value(PARTICLES.get(), entity, what, set, value) }
}

unsafe extern "C" fn smoke_value(entity: u64, what: i32, set: i32, value: *mut [f32; 4]) -> i32 {
    unsafe { component_value(SMOKES.get(), entity, what, set, value) }
}

unsafe extern "C" fn mist_value(entity: u64, what: i32, set: i32, value: *mut [f32; 4]) -> i32 {
    unsafe { component_value(MISTS.get(), entity, what, set, value) }
}

unsafe extern "C" fn motion_matcher_value(entity: u64, what: i32, set: i32, value: *mut [f32; 4]) -> i32 {
    unsafe { component_value(MATCHERS.get(), entity, what, set, value) }
}

unsafe extern "C" fn bone_attachment_value(entity: u64, what: i32, set: i32, value: *mut [f32; 4]) -> i32 {
    unsafe { component_value(ATTACHMENTS.get(), entity, what, set, value) }
}

/// Answers a script's read, or write with `set`, of value `what` through
/// `values`: 1 when the Obj has the component, the value read put in
/// `value`.
unsafe fn component_value(values: Option<&ComponentValues>, entity: u64, what: i32, set: i32, value: *mut [f32; 4]) -> i32 {
    let (Some(values), false) = (values, value.is_null()) else { return 0 };
    let value = unsafe { &mut *value };
    let written = (set != 0).then_some(*value);
    let done = with_world(|world| values(world, Entity::from_bits(entity), what, written));
    match done {
        Some(read) => {
            if written.is_none() {
                *value = read;
            }
            1
        }
        None => 0,
    }
}

static MOB_MOTIONS: std::sync::OnceLock<ComponentValues> = std::sync::OnceLock::new();
static BLOCK_MOBS: std::sync::OnceLock<ComponentValues> = std::sync::OnceLock::new();

/// Sets what answers `MobMotion` in scripts; the first set is kept.
pub fn set_mob_motion_values(values: ComponentValues) {
    let _ = MOB_MOTIONS.set(values);
}

/// Sets what answers `BlockMob` in scripts; the first set is kept.
pub fn set_block_mob_values(values: ComponentValues) {
    let _ = BLOCK_MOBS.set(values);
}

unsafe extern "C" fn mob_motion_value(entity: u64, what: i32, set: i32, value: *mut [f32; 4]) -> i32 {
    unsafe { component_value(MOB_MOTIONS.get(), entity, what, set, value) }
}

unsafe extern "C" fn block_mob_value(entity: u64, what: i32, set: i32, value: *mut [f32; 4]) -> i32 {
    unsafe { component_value(BLOCK_MOBS.get(), entity, what, set, value) }
}

unsafe extern "C" fn particle_blast(at: *const [f32; 3], radius: f32, speed: f32) -> i32 {
    if at.is_null() {
        return 0;
    }
    let at = unsafe { *at };
    let set_off = BLASTER.get().and_then(|blaster| with_world(|world| Some(blaster(world, at, radius, speed))));
    i32::from(set_off.is_some())
}

unsafe extern "C" fn water_splash(at: *const [f32; 3], size: f32) -> i32 {
    if at.is_null() {
        return 0;
    }
    let at = unsafe { *at };
    let made = SPLASHER.get().and_then(|splasher| with_world(|world| Some(splasher(world, at, size))));
    i32::from(made == Some(true))
}

unsafe extern "C" fn water_sample(at: *const [f32; 3], out: *mut [f32; 7]) -> i32 {
    if at.is_null() || out.is_null() {
        return 0;
    }
    let at = unsafe { *at };
    let found = WATER.get().and_then(|sampler| with_world(|world| sampler(world, at)));
    match found {
        Some(values) => {
            unsafe { *out = values };
            1
        }
        None => 0,
    }
}

/// `behaviours_adopt` state bits, set by `Activation` in Voxear.Core.
const ADOPT_ENABLED: u32 = 1;
const ADOPT_WANTS_ENABLED: u32 = 2;
const ADOPT_DESTROYED: u32 = 4;

/// Type name, entity, field values as JSON (or null), whether it starts
/// enabled, and the info to fill.
pub(crate) type CreateBehaviour =
    unsafe extern "C" fn(*const u8, i32, u64, *const u8, i32, i32, *mut BehaviourInfo) -> i32;

/// Type name, entity, and field values as JSON (or null).
pub(crate) type AddComponent = unsafe extern "C" fn(*const u8, i32, u64, *const u8, i32) -> i32;

/// Script functions the engine calls; filled in by `Bootstrap.Initialize`.
#[repr(C)]
#[derive(Default)]
pub(crate) struct ManagedApi {
    pub size: u32,
    pub version: u32,
    pub load_game_assembly: Option<unsafe extern "C" fn(path: *const u8, len: i32) -> i32>,
    pub create_behaviour: Option<CreateBehaviour>,
    pub invoke: Option<unsafe extern "C" fn(phase: i32, handles: *const u32, count: i32)>,
    pub release: Option<unsafe extern "C" fn(handles: *const u32, count: i32)>,
    /// Once per frame, before anything else: resumes awaits whose loads
    /// finished and runs continuations posted to the main thread.
    pub update: Option<unsafe extern "C" fn()>,
    /// Writes the `[ContentRoot]` patterns into a buffer when they fit, and
    /// returns their length either way, or -1 on failure.
    pub content_roots: Option<unsafe extern "C" fn(buffer: *mut u8, capacity: i32) -> i32>,
    /// Starts a level change as `Level.LoadAsync` does, for the editor, so
    /// scripts hear about it as they would about their own.
    pub switch_level: Option<unsafe extern "C" fn(level: *const u8, len: i32)>,
    /// Runs a `[Command]` method, for the editor's palette: 0 when it ran,
    /// 1 when it was not found, 2 when it threw.
    pub run_command:
        Option<unsafe extern "C" fn(ty: *const u8, ty_len: i32, method: *const u8, method_len: i32) -> i32>,
    /// Loads a new build of the game scripts in place of the running one,
    /// given each scheduled behaviour's handle and whether it is enabled, and
    /// fills in each one's info in the new build: 0 when it reloaded, 1 when
    /// the build could not be loaded and nothing changed, 2 when it failed
    /// partway.
    pub reload_game_assembly: Option<
        unsafe extern "C" fn(
            path: *const u8,
            len: i32,
            handles: *const u32,
            enabled: *const u32,
            infos: *mut BehaviourInfo,
            count: i32,
        ) -> i32,
    >,
    /// Runs the C# systems of one stage, by its index in the engine's
    /// stages, in order.
    pub run_systems: Option<unsafe extern "C" fn(stage: i32)>,
    /// Gives an entity a C# component by type name, with field values as
    /// JSON (or null): 0 when it did, -1 when it could not.
    pub add_component: Option<AddComponent>,
    /// Runs a line typed in a development build's console, printing what it
    /// did as log lines: 0 when it ran, 1 when nothing has its first word as
    /// a name, 2 when it failed.
    pub console_run: Option<unsafe extern "C" fn(line: *const u8, len: i32) -> i32>,
    /// The console's commands and variables as "name\tusage\thelp" lines,
    /// written when they fit, and their length either way.
    pub console_names: Option<unsafe extern "C" fn(buffer: *mut u8, capacity: i32) -> i32>,
    /// Runs a check a guide's step names: 0 when it passes; 1 when it does
    /// not, with what is still wrong written into the buffer, cut to fit,
    /// and its length into `written`; 2 when there is no such check; 3 when
    /// it threw.
    pub run_check: Option<RunCheck>,
    /// Answers the Inspector about a behaviour in edit mode, as JSON (see
    /// `inspector`), writing the answer when it fits and returning its
    /// length either way, or -1. A request of no bytes asks for the last
    /// answer again.
    pub inspector_call:
        Option<unsafe extern "C" fn(request: *const u8, len: i32, buffer: *mut u8, capacity: i32) -> i32>,
    /// The managed heap's numbers, as many as fit: see
    /// [`ManagedMemory`](crate::ManagedMemory).
    pub managed_memory: Option<unsafe extern "C" fn(values: *mut u64, count: i32) -> i32>,
    /// Tells each Obj's behaviours of the last physics step's contacts and
    /// trigger crossings (`OnCollisionEnter` and the rest), each event
    /// once for each Obj in it.
    pub physics_events: Option<unsafe extern "C" fn(events: *const crate::physics::PhysicsEventData, count: i32)>,
    /// Tells behaviours of the marks actions passed (`OnMotionMark`): each
    /// mark's Obj, and the marks one to a line.
    pub motion_marks: Option<unsafe extern "C" fn(entities: *const u64, count: i32, text: *const u8, len: i32)>,
    /// Sets the replicated properties that came from the server on their
    /// behaviours now, rather than with the next frame's events, as a
    /// predicted Obj put right needs before its ticks run again.
    pub take_net_fields: Option<unsafe extern "C" fn()>,
    /// Enables or disables a behaviour as a script setting its `Enabled`
    /// does, for the editor's tick: the engine hears of it as a request.
    pub set_enabled: Option<unsafe extern "C" fn(handle: u32, enabled: i32)>,
    /// Tells scripts the world's origin moved, and how far every Obj moved,
    /// in whole metres: fields marked `[WorldPosition]` move too, then
    /// `Origin.Moved` is raised.
    pub origin_moved: Option<unsafe extern "C" fn(by: *const i32)>,
    /// Tells scripts of particles' strikes (`OnParticleHit` on the
    /// behaviours of each system's Obj, and `Particles.Hit`).
    pub particle_hits: Option<unsafe extern "C" fn(hits: *const crate::physics::ParticleHitData, count: i32)>,
    /// Tells scripts of Heat Bodies' changes and characters' feelings
    /// (`HeatBody.Changed` and `HeatBody.FeelsChanged`), with each change's
    /// name one to a line.
    pub heat_events:
        Option<unsafe extern "C" fn(events: *const crate::heat::HeatEventData, count: i32, text: *const u8, len: i32)>,
}

pub(crate) type RunCheck = unsafe extern "C" fn(
    ty: *const u8,
    ty_len: i32,
    method: *const u8,
    method_len: i32,
    buffer: *mut u8,
    capacity: i32,
    written: *mut i32,
) -> i32;

#[repr(C)]
#[derive(Default, Debug)]
pub(crate) struct BehaviourInfo {
    pub callbacks: u32,
    pub order: i32,
    pub flags: u32,
}

pub(crate) const FLAG_INSTANCE: u32 = 1;
/// After a reload: the behaviour's class is not in the new build.
pub(crate) const FLAG_GONE: u32 = 2;

thread_local! {
    static WORLD: Cell<*mut World> = const { Cell::new(ptr::null_mut()) };
    static REQUESTS: RefCell<Vec<Request>> = const { RefCell::new(Vec::new()) };
    static LEVEL_REQUESTS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static LIGHTING_REQUESTS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static LOOK_PACK_REQUESTS: RefCell<Vec<(String, bool)>> = const { RefCell::new(Vec::new()) };
    static TEXTURE_PACK_REQUESTS: RefCell<Vec<(String, bool)>> = const { RefCell::new(Vec::new()) };
    /// The level's time of year, as `Seasons.TimeOfYear` reads it: NaN
    /// without Seasons. Scripts setting it change it here at once, and the
    /// engine takes what they set after the frame.
    static TIME_OF_YEAR: Cell<f32> = const { Cell::new(f32::NAN) };
    static TIME_OF_YEAR_SET: Cell<Option<f32>> = const { Cell::new(None) };
    /// The level's weather as `Weather` reads it: its kind, what it adds this moment and whether it is turning.
    /// Scripts turning it change the kind here at once, and the engine takes what they asked after the frame.
    static WEATHER: Cell<WeatherState> =
        const { Cell::new(WeatherState { kind: 0, state: [0.0; 11], turning: false }) };
    static WEATHER_SET: Cell<Option<WeatherRequest>> = const { Cell::new(None) };
    /// Which of the sea's costlier looks are on, as `Water.Features` reads
    /// them: none without an Ocean. Scripts setting them change them here at
    /// once, and the engine takes what they set after the frame.
    static WATER_FEATURES: Cell<Option<u32>> = const { Cell::new(None) };
    static WATER_FEATURES_SET: Cell<Option<u32>> = const { Cell::new(None) };
    static SCENE_REQUESTS: RefCell<Vec<SceneRequest>> = const { RefCell::new(Vec::new()) };
    static OBJ_DESTROYS: RefCell<Vec<Entity>> = const { RefCell::new(Vec::new()) };
    /// Managed `CreateBehaviour` and `AddComponent`, for behaviours and
    /// components made during script calls.
    static CREATE: Cell<Option<(CreateBehaviour, AddComponent)>> = const { Cell::new(None) };
    /// Behaviours created during script calls that scripts are still
    /// activating, until `behaviours_adopt` hands them to the lifecycle.
    static ACTIVATING: RefCell<HashMap<u32, Registration>> = RefCell::new(HashMap::new());
    /// The behaviours the last `prefab_instantiate` created, for `batch_handles`.
    static LAST_BATCH: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn set_create_behaviour(create: CreateBehaviour, add: AddComponent) {
    CREATE.set(Some((create, add)));
}

pub(crate) fn take_obj_destroys() -> Vec<Entity> {
    OBJ_DESTROYS.take()
}

/// Lends the world to native callbacks for the duration of a script call.
pub(crate) struct WorldScope(*mut World, u64);

impl WorldScope {
    pub fn enter(world: &mut World) -> Self {
        let id = CURRENT_WORLD.swap(world.id(), Ordering::Relaxed);
        Self(WORLD.replace(world), id)
    }
}

impl Drop for WorldScope {
    fn drop(&mut self) {
        WORLD.set(self.0);
        CURRENT_WORLD.store(self.1, Ordering::Relaxed);
    }
}

pub(crate) fn drain_requests(out: &mut Vec<Request>) {
    REQUESTS.with_borrow_mut(|requests| out.append(requests));
}

pub(crate) fn take_level_requests() -> Vec<String> {
    LEVEL_REQUESTS.take()
}

pub(crate) fn take_lighting_requests() -> Vec<String> {
    LIGHTING_REQUESTS.take()
}

pub(crate) fn take_look_pack_requests() -> Vec<(String, bool)> {
    LOOK_PACK_REQUESTS.take()
}

pub(crate) fn take_texture_pack_requests() -> Vec<(String, bool)> {
    TEXTURE_PACK_REQUESTS.take()
}

/// The level's weather as scripts read it: its kind, by its place among the kinds; what it adds this moment
/// (cloud, darkness, fog, wind, rain, snow, lightning, dust), then how wet the ground is, how much snow has settled and
/// how bright its lightning is; and whether it is still turning.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeatherState {
    pub kind: i32,
    pub state: [f32; 11],
    pub turning: bool,
}

/// What a script asked of the weather: to turn over `seconds` to a kind, or with `amounts` to those.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeatherRequest {
    pub kind: i32,
    pub amounts: Option<[f32; 8]>,
    pub seconds: f32,
}

pub(crate) fn publish_weather(state: WeatherState) {
    WEATHER.set(state);
}

pub(crate) fn take_weather() -> Option<WeatherRequest> {
    WEATHER_SET.take()
}

unsafe extern "C" fn weather_state(written: *mut [f32; 11], turning: *mut i32) -> i32 {
    let state = WEATHER.get();
    if !written.is_null() {
        unsafe { *written = state.state };
    }
    if !turning.is_null() {
        unsafe { *turning = i32::from(state.turning) };
    }
    state.kind
}

unsafe extern "C" fn weather_set(kind: i32, amounts: *const [f32; 8], seconds: f32) {
    let amounts = (!amounts.is_null()).then(|| unsafe { *amounts });
    WEATHER.set(WeatherState { kind, turning: seconds > 0.0, ..WEATHER.get() });
    let seconds = if seconds.is_finite() { seconds.max(0.0) } else { 0.0 };
    WEATHER_SET.set(Some(WeatherRequest { kind, amounts, seconds }));
}

pub(crate) fn publish_time_of_year(time: Option<f32>) {
    TIME_OF_YEAR.set(time.unwrap_or(f32::NAN));
}

pub(crate) fn take_time_of_year() -> Option<f32> {
    TIME_OF_YEAR_SET.take()
}

pub(crate) fn publish_water_features(features: Option<u32>) {
    WATER_FEATURES.set(features);
}

pub(crate) fn take_water_features() -> Option<u32> {
    WATER_FEATURES_SET.take()
}

pub(crate) fn take_scene_requests() -> Vec<SceneRequest> {
    SCENE_REQUESTS.take()
}

pub(crate) fn with_world<R>(f: impl FnOnce(&mut World) -> Option<R>) -> Option<R> {
    let world = WORLD.get();
    if world.is_null() {
        return None;
    }
    f(unsafe { &mut *world })
}

pub(crate) unsafe fn utf8<'a>(ptr: *const u8, len: i32) -> std::borrow::Cow<'a, str> {
    if ptr.is_null() || len <= 0 {
        return "".into();
    }
    String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(ptr, len as usize) })
}

#[allow(clippy::too_many_arguments)]
unsafe extern "C" fn log(
    level: i32,
    message: *const u8,
    len: i32,
    stack: *const u8,
    stack_len: i32,
    context: u64,
    prefab: i32,
    asset: *const u8,
    asset_len: i32,
) {
    let text = unsafe { utf8(message, len) };
    let stack = if stack.is_null() { Default::default() } else { unsafe { utf8(stack, stack_len) } };
    let stack: &str = &stack;
    // A prefab is named by the file it was loaded from, as the Project panel shows it.
    let asset = match prefab {
        prefab if prefab >= 0 => with_services(|services| services.prefab_path(prefab as u32)).flatten(),
        _ => Some(unsafe { utf8(asset, asset_len) }.into_owned()),
    };
    let asset: &str = asset.as_deref().unwrap_or_default();
    match level {
        0 => tracing::debug!(target: "script", stack, context, asset, "{text}"),
        1 => tracing::info!(target: "script", stack, context, asset, "{text}"),
        2 => tracing::warn!(target: "script", stack, context, asset, "{text}"),
        _ => tracing::error!(target: "script", stack, context, asset, "{text}"),
    }
}

unsafe extern "C" fn obj_find(name: *const u8, len: i32, out: *mut u64, capacity: i32) -> i32 {
    let name = unsafe { utf8(name, len) };
    let found = with_world(|world| {
        let mut found: Vec<Entity> = world
            .query_ref::<(Entity, &Name)>()
            .without::<voxear_scene::EditorOnly>()
            .without::<voxear_scene::Destroying>()
            .into_iter()
            .filter(|(_, n)| n.0 == name)
            .map(|(entity, _)| entity)
            .collect();
        found.sort_by_key(|entity| entity.index());
        Some(found)
    })
    .unwrap_or_default();
    if !out.is_null() {
        for (at, entity) in found.iter().take(capacity.max(0) as usize).enumerate() {
            unsafe { *out.add(at) = entity.to_bits() };
        }
    }
    found.len() as i32
}

unsafe extern "C" fn save_key(
    kind: i32,
    id: u64,
    reference: *const u8,
    len: i32,
    buffer: *mut u8,
    capacity: i32,
) -> i32 {
    let reference = unsafe { utf8(reference, len) };
    let key = match kind {
        0 => with_services(|services| with_world(|world| services.obj_key(world, Entity::from_bits(id)))),
        1 => with_services(|services| services.asset_guid(AssetName::Reference(&reference))),
        2 => with_services(|services| services.asset_guid(AssetName::Prefab(id as u32))),
        3 => with_services(|services| services.asset_guid(AssetName::Sound(id as u32))),
        _ => None,
    };
    let Some(key) = key.flatten() else { return -1 };
    if !buffer.is_null() && key.len() <= capacity.max(0) as usize {
        unsafe { ptr::copy_nonoverlapping(key.as_ptr(), buffer, key.len()) };
    }
    key.len() as i32
}

unsafe extern "C" fn save_obj(key: *const u8, len: i32) -> u64 {
    let key = unsafe { utf8(key, len) };
    with_services(|services| with_world(|world| services.obj_by_key(world, &key))).flatten().map_or(0, Entity::to_bits)
}

unsafe extern "C" fn save_scene(buffer: *mut u8, capacity: i32) -> i32 {
    let doomed = OBJ_DESTROYS.with_borrow(Clone::clone);
    let Some(state) = with_services(|services| with_world(|world| services.scene_state(world, &doomed))).flatten()
    else {
        return -1;
    };
    if !buffer.is_null() && state.len() <= capacity.max(0) as usize {
        unsafe { ptr::copy_nonoverlapping(state.as_ptr(), buffer, state.len()) };
    }
    state.len() as i32
}

unsafe extern "C" fn preview_create(
    prefab: i32,
    reference: *const u8,
    len: i32,
    options: *const PreviewOptions,
) -> i32 {
    let reference = unsafe { utf8(reference, len) };
    let source = if prefab >= 0 { PreviewSource::Prefab(prefab as u32) } else { PreviewSource::Asset(&reference) };
    let Some(options) = (unsafe { options.as_ref() }).map(|o| unsafe { ptr::read_unaligned(o) }) else { return -1 };
    match with_services(|services| services.create_preview(source, &options)) {
        Some(Ok(id)) => id as i32,
        Some(Err(error)) => {
            tracing::error!(target: "script", "Preview.Of({source:?}): {error:#}");
            -1
        }
        None => {
            tracing::error!(target: "script", "Preview.Of({source:?}): previews are not available here");
            -1
        }
    }
}

unsafe extern "C" fn preview_state(preview: i32, out: *mut PreviewState) -> i32 {
    let Some(state) = with_services(|services| services.preview_state(preview as u32)).flatten() else { return -1 };
    if !out.is_null() {
        unsafe { ptr::write_unaligned(out, state) };
    }
    state.ready as i32
}

unsafe extern "C" fn preview_save(preview: i32, path: *const u8, len: i32) -> i32 {
    let path = unsafe { utf8(path, len) };
    match with_services(|services| services.save_preview(preview as u32, &path)) {
        Some(Ok(())) => 0,
        Some(Err(error)) => {
            tracing::error!(target: "script", "Preview.Save(\"{path}\"): {error:#}");
            -1
        }
        None => -1,
    }
}

unsafe extern "C" fn preview_free(preview: i32) {
    with_services(|services| services.free_preview(preview as u32));
}

unsafe extern "C" fn profiler_marker(name: *const u8, len: i32) -> u32 {
    Marker::new(&unsafe { utf8(name, len) }).id()
}

unsafe extern "C" fn profiler_begin(marker: u32) -> i32 {
    i32::from(Marker::from_id(marker).begin())
}

unsafe extern "C" fn profiler_end() {
    profiling::end();
}

unsafe extern "C" fn profiler_end_alloc(bytes: u64) {
    profiling::end_with_alloc(bytes);
}

unsafe extern "C" fn profiler_enabled() -> i32 {
    i32::from(profiling::enabled())
}

unsafe extern "C" fn obj_name(entity: u64, buffer: *mut u8, capacity: i32) -> i32 {
    let name = with_world(|world| world.get::<Name>(Entity::from_bits(entity)).map(|n| n.0.clone()));
    let Some(name) = name else { return -1 };
    let count = name.len().min(capacity.max(0) as usize);
    if !buffer.is_null() {
        unsafe { ptr::copy_nonoverlapping(name.as_ptr(), buffer, count) };
    }
    name.len() as i32
}

unsafe extern "C" fn transform_get(entity: u64, out: *mut Transform) -> i32 {
    let transform = with_world(|world| world.get::<Transform>(Entity::from_bits(entity)).copied());
    match transform {
        Some(transform) => {
            unsafe { out.write_unaligned(transform) };
            1
        }
        None => 0,
    }
}

unsafe extern "C" fn transform_set(entity: u64, value: *const Transform) -> i32 {
    let value = unsafe { value.read_unaligned() };
    let written = with_world(|world| world.get_mut::<Transform>(Entity::from_bits(entity)).map(|t| *t = value));
    i32::from(written.is_some())
}

unsafe extern "C" fn behaviour_set_enabled(handle: u32, enabled: i32) {
    REQUESTS.with_borrow_mut(|r| r.push(Request::SetEnabled(BehaviourHandle(handle), enabled != 0)));
}

unsafe extern "C" fn behaviour_destroy(handle: u32) {
    REQUESTS.with_borrow_mut(|r| r.push(Request::Destroy(BehaviourHandle(handle))));
}

unsafe extern "C" fn level_load(level: *const u8, len: i32) {
    let level = unsafe { utf8(level, len) }.into_owned();
    LEVEL_REQUESTS.with_borrow_mut(|r| r.push(level));
}

unsafe extern "C" fn lighting_apply(reference: *const u8, len: i32) {
    let reference = unsafe { utf8(reference, len) }.into_owned();
    LIGHTING_REQUESTS.with_borrow_mut(|r| r.push(reference));
}

unsafe extern "C" fn look_pack_wear(reference: *const u8, len: i32, remember: i32) {
    let reference = unsafe { utf8(reference, len) }.into_owned();
    LOOK_PACK_REQUESTS.with_borrow_mut(|r| r.push((reference, remember != 0)));
}

unsafe extern "C" fn texture_pack_wear(reference: *const u8, len: i32, remember: i32) {
    let reference = unsafe { utf8(reference, len) }.into_owned();
    TEXTURE_PACK_REQUESTS.with_borrow_mut(|r| r.push((reference, remember != 0)));
}

unsafe extern "C" fn seasons_time() -> f32 {
    TIME_OF_YEAR.get()
}

unsafe extern "C" fn water_features() -> i32 {
    WATER_FEATURES.get().map_or(-1, |features| features as i32)
}

unsafe extern "C" fn water_set_features(features: i32) {
    if WATER_FEATURES.get().is_none() {
        tracing::warn!(target: "script", "Water.Features: the level has no Ocean");
        return;
    }
    let features = features.max(0) as u32;
    WATER_FEATURES.set(Some(features));
    WATER_FEATURES_SET.set(Some(features));
}

unsafe extern "C" fn seasons_set_time(time: f32) {
    if TIME_OF_YEAR.get().is_nan() {
        tracing::warn!(target: "script", "Seasons.TimeOfYear: the level has no Seasons to turn");
        return;
    }
    let time = time.rem_euclid(4.0);
    TIME_OF_YEAR.set(time);
    TIME_OF_YEAR_SET.set(Some(time));
}

unsafe extern "C" fn load_async(level: i32, reference: *const u8, len: i32) -> i32 {
    let reference = unsafe { utf8(reference, len) };
    let what = if level != 0 { "Level.LoadAsync" } else { "Prefab.LoadAsync" };
    match with_services(|services| services.load_async(level != 0, &reference)) {
        Some(Ok(id)) => id as i32,
        Some(Err(error)) => {
            tracing::error!(target: "script", "{what}(\"{reference}\"): {error:#}");
            -1
        }
        None => {
            tracing::error!(target: "script", "{what}(\"{reference}\"): content is not available here");
            -1
        }
    }
}

unsafe extern "C" fn level_load_async(reference: *const u8, len: i32, options: *const LevelLoadOptions) -> i32 {
    let reference = unsafe { utf8(reference, len) };
    let options = unsafe { options.read_unaligned() };
    match with_services(|services| services.load_level(&reference, options)) {
        Some(Ok(id)) => id as i32,
        Some(Err(error)) => {
            tracing::error!(target: "script", "Level.LoadAsync(\"{reference}\"): {error:#}");
            -1
        }
        None => {
            tracing::error!(target: "script", "Level.LoadAsync(\"{reference}\"): levels are not available here");
            -1
        }
    }
}

unsafe extern "C" fn scene_load_async(reference: *const u8, len: i32, options: *const SceneLoadOptions) -> i32 {
    let reference = unsafe { utf8(reference, len) };
    let options = unsafe { options.read_unaligned() };
    match with_services(|services| services.load_scene(&reference, options)) {
        Some(Ok(id)) => id as i32,
        Some(Err(error)) => {
            tracing::error!(target: "script", "Scene.LoadAsync(\"{reference}\"): {error:#}");
            -1
        }
        None => {
            tracing::error!(target: "script", "Scene.LoadAsync(\"{reference}\"): scenes are not available here");
            -1
        }
    }
}

unsafe extern "C" fn scene_unload(reference: *const u8, len: i32) {
    let scene = unsafe { utf8(reference, len) }.into_owned();
    SCENE_REQUESTS.with_borrow_mut(|requests| requests.push(SceneRequest::Unload(scene)));
}

unsafe extern "C" fn scene_set_active(reference: *const u8, len: i32) {
    let scene = unsafe { utf8(reference, len) }.into_owned();
    SCENE_REQUESTS.with_borrow_mut(|requests| requests.push(SceneRequest::SetActive(scene)));
}

/// Writes what was asked into the buffer when it fits, and returns its
/// length in bytes either way, or -1 for nothing to say.
unsafe extern "C" fn level_info(what: i32, id: i32, buffer: *mut u8, capacity: i32) -> i32 {
    let Some(what) = LevelInfo::from_code(what) else { return -1 };
    let Some(text) = with_services(|services| services.level_info(what, id.max(0) as u32)).flatten() else {
        return -1;
    };
    if !buffer.is_null() && text.len() <= capacity.max(0) as usize {
        unsafe { ptr::copy_nonoverlapping(text.as_ptr(), buffer, text.len()) };
    }
    text.len() as i32
}

unsafe extern "C" fn load_status(id: u32, out: *mut LoadProgress) -> i32 {
    match with_services(|services| services.load_status(id)).flatten() {
        Some(status) => {
            unsafe { out.write_unaligned(status) };
            1
        }
        None => 0,
    }
}

unsafe extern "C" fn load_allow_activation(id: u32, allow: i32) {
    with_services(|services| services.allow_activation(id, allow != 0));
}

unsafe extern "C" fn prefab_keep(prefab: i32, keep: i32) {
    if prefab >= 0 {
        with_services(|services| services.keep_prefab(prefab as u32, keep != 0));
    }
}

unsafe extern "C" fn content_unload_unused() {
    with_services(|services| services.unload_unused());
}

unsafe extern "C" fn content_usage(out: *mut ContentStats) -> i32 {
    match with_services(|services| services.usage()) {
        Some(stats) => {
            unsafe { out.write_unaligned(stats) };
            1
        }
        None => 0,
    }
}

/// The Obj's transform in the world as of now, through its parents.
unsafe extern "C" fn obj_world_transform(entity: u64, out: *mut Transform) -> i32 {
    let entity = Entity::from_bits(entity);
    let matrix = with_world(|world| world.is_alive(entity).then(|| world_matrix(world, entity)));
    let Some(matrix) = matrix else { return 0 };
    let (scale, rotation, position) = matrix.to_scale_rotation_translation();
    unsafe { out.write_unaligned(Transform::new(position, rotation, scale)) };
    1
}

unsafe extern "C" fn world_origin(out: *mut i64) {
    let origin = with_world(|world| Some(voxear_scene::WorldOrigin::of(world))).unwrap_or_default();
    for (axis, metres) in origin.0.to_array().into_iter().enumerate() {
        unsafe { out.add(axis).write(metres) };
    }
}

unsafe extern "C" fn obj_destroy(entity: u64) {
    push_obj_destroy(Entity::from_bits(entity));
}

/// Destroys an Obj, with its children and behaviours, once the script call
/// returns.
pub(crate) fn push_obj_destroy(entity: Entity) {
    OBJ_DESTROYS.with_borrow_mut(|d| d.push(entity));
}

unsafe extern "C" fn obj_behaviours(entity: u64, out: *mut u32, capacity: i32) -> i32 {
    let handles: Option<Vec<u32>> = with_world(|world| {
        world.get::<Behaviours>(Entity::from_bits(entity)).map(|b| b.handles().map(|h| h.0).collect())
    });
    let handles = handles.unwrap_or_default();
    if !out.is_null() {
        let count = handles.len().min(capacity.max(0) as usize);
        unsafe { ptr::copy_nonoverlapping(handles.as_ptr(), out, count) };
    }
    handles.len() as i32
}

unsafe extern "C" fn behaviour_add(entity: u64, type_name: *const u8, len: i32) -> i32 {
    let type_name = unsafe { utf8(type_name, len) }.into_owned();
    let entity = Entity::from_bits(entity);
    if !with_world(|world| Some(world.is_alive(entity))).unwrap_or(false) {
        tracing::error!(target: "script", "AddBehaviour<{type_name}>: the Obj no longer exists");
        return -1;
    }
    attach(entity, &[BehaviourSpec::new(&type_name)]).first().map_or(-1, |&handle| handle as i32)
}

unsafe extern "C" fn prefab_load(reference: *const u8, len: i32) -> i32 {
    let reference = unsafe { utf8(reference, len) };
    match with_services(|services| services.load_prefab(&reference)) {
        Some(Ok(id)) => id as i32,
        Some(Err(error)) => {
            tracing::error!(target: "script", "Prefab.Load(\"{reference}\"): {error:#}");
            -1
        }
        None => {
            tracing::error!(target: "script", "Prefab.Load(\"{reference}\"): content is not available here");
            -1
        }
    }
}

unsafe extern "C" fn prefab_instantiate(
    prefab: i32,
    placement: *const Transform,
    world_space: i32,
    parent: u64,
    out_root: *mut u64,
) -> i32 {
    let spawn = |services: &mut dyn ScriptServices, world: &mut World, parent, placement| {
        services.spawn_prefab(world, prefab as u32, parent, placement)
    };
    unsafe { instantiate("prefabs can only be instantiated", placement, world_space, parent, out_root, spawn) }
}

unsafe extern "C" fn obj_instantiate(
    original: u64,
    placement: *const Transform,
    world_space: i32,
    parent: u64,
    out_root: *mut u64,
) -> i32 {
    let original = Entity::from_bits(original);
    let copy = |services: &mut dyn ScriptServices, world: &mut World, parent, placement| {
        services.copy_obj(world, original, parent, placement)
    };
    unsafe { instantiate("Objs can only be copied", placement, world_space, parent, out_root, copy) }
}

/// Makes Objs through `spawn`, a prefab's or a copy of an Obj, under
/// `parent` at `placement`, given in the world when `world_space` is set,
/// and creates their behaviours. Returns how many, for `batch_handles`, or
/// -1 when nothing was made.
pub(crate) unsafe fn instantiate(
    only_while: &str,
    placement: *const Transform,
    world_space: i32,
    parent: u64,
    out_root: *mut u64,
    spawn: impl FnOnce(&mut dyn ScriptServices, &mut World, Option<Entity>, Option<Placement>) -> anyhow::Result<SpawnedPrefab>,
) -> i32 {
    // Generation zero is the null entity, which is Obj.None.
    let parent = (parent >> 32 != 0).then(|| Entity::from_bits(parent));
    let placement = (!placement.is_null()).then(|| {
        let transform = unsafe { placement.read_unaligned() };
        Placement { position: transform.position, rotation: transform.rotation }
    });
    let spawned = with_services(|services| {
        with_world(|world| {
            let placement = match (placement, parent) {
                (Some(placement), Some(parent)) if world_space != 0 => Some(to_local(world, parent, placement)),
                (placement, _) => placement,
            };
            Some(spawn(services, world, parent, placement))
        })
    })
    .flatten();
    let SpawnedPrefab { root, objs } = match spawned {
        Some(Ok(spawned)) => spawned,
        Some(Err(error)) => {
            tracing::error!(target: "script", "Instantiate: {error:#}");
            return -1;
        }
        None => {
            tracing::error!(target: "script", "Instantiate: {only_while} while scripts run");
            return -1;
        }
    };
    // Behaviours are created only now, outside the world borrow, because
    // creating one runs managed code that may call back into the world.
    let handles: Vec<u32> = objs.iter().flat_map(|(entity, specs)| attach(*entity, specs)).collect();
    let count = handles.len() as i32;
    LAST_BATCH.set(handles);
    unsafe { out_root.write_unaligned(root.to_bits()) };
    count
}

unsafe extern "C" fn batch_handles(out: *mut u32, capacity: i32) -> i32 {
    LAST_BATCH.with_borrow(|handles| {
        if !out.is_null() {
            let count = handles.len().min(capacity.max(0) as usize);
            unsafe { ptr::copy_nonoverlapping(handles.as_ptr(), out, count) };
        }
        handles.len() as i32
    })
}

unsafe extern "C" fn behaviours_adopt(handles: *const u32, states: *const u32, count: i32) {
    if handles.is_null() || states.is_null() || count <= 0 {
        return;
    }
    let handles = unsafe { std::slice::from_raw_parts(handles, count as usize) };
    let states = unsafe { std::slice::from_raw_parts(states, count as usize) };
    let adopted: Vec<Request> = ACTIVATING.with_borrow_mut(|activating| {
        handles
            .iter()
            .zip(states)
            .filter_map(|(handle, state)| {
                let registration = activating.remove(handle)?;
                let registration = Registration { enabled: state & ADOPT_WANTS_ENABLED != 0, ..registration };
                let adopted = Adopted { enabled: state & ADOPT_ENABLED != 0, destroyed: state & ADOPT_DESTROYED != 0 };
                Some(Request::Adopt(registration, adopted))
            })
            .collect()
    });
    REQUESTS.with_borrow_mut(|requests| requests.extend(adopted));
}

/// Creates behaviours on an Obj during a script call, after its C#
/// components. Scripts then run their Awake and OnEnable and hand them back
/// through `behaviours_adopt`.
fn attach(entity: Entity, specs: &[BehaviourSpec]) -> Vec<u32> {
    let Some((create, add)) = CREATE.get() else { return Vec::new() };
    let (components, specs): (Vec<&BehaviourSpec>, Vec<&BehaviourSpec>) = specs.iter().partition(|s| s.component);
    for spec in &components {
        let added = call_with_fields(spec, |name, len, fields, fields_len| unsafe {
            add(name, len, entity.to_bits(), fields, fields_len)
        });
        if added != 0 {
            tracing::error!(target: "script", "could not add component `{}`; is it an IComponent struct?", spec.type_name);
        }
    }
    with_world(|world| {
        SceneComponents::record(world, entity, components.into_iter().cloned());
        Some(())
    });
    let mut attached = Vec::with_capacity(specs.len());
    let mut is_instance = false;
    for spec in specs {
        let mut info = BehaviourInfo::default();
        let Some(handle) = call_create(create, spec, entity, &mut info) else {
            tracing::error!(
                target: "script",
                "could not create behaviour `{}`; is it a Behaviour in the game assembly?",
                spec.type_name
            );
            continue;
        };
        let registration = Registration {
            handle,
            entity,
            callbacks: Callbacks(info.callbacks),
            order: info.order,
            enabled: spec.enabled,
        };
        ACTIVATING.with_borrow_mut(|activating| activating.insert(handle.0, registration));
        is_instance |= info.flags & FLAG_INSTANCE != 0;
        attached.push(AttachedBehaviour { handle, spec: (*spec).clone() });
    }
    if attached.is_empty() {
        return Vec::new();
    }
    let handles = attached.iter().map(|b| b.handle.0).collect();
    with_world(|world| {
        match world.get_mut::<Behaviours>(entity) {
            Some(behaviours) => behaviours.0.extend(attached),
            None => {
                world.insert(entity, Behaviours(attached));
            }
        }
        if is_instance {
            world.insert(entity, InstanceMarker);
        }
        Some(())
    });
    handles
}

/// Calls managed `CreateBehaviour`, passing the spec's fields as JSON, which
/// .NET reads without extra packages.
pub(crate) fn call_create(
    create: CreateBehaviour,
    spec: &BehaviourSpec,
    entity: Entity,
    info: &mut BehaviourInfo,
) -> Option<BehaviourHandle> {
    let enabled = i32::from(spec.enabled);
    let handle = call_with_fields(spec, |name, len, fields, fields_len| unsafe {
        create(name, len, entity.to_bits(), fields, fields_len, enabled, info)
    });
    (handle >= 0).then_some(BehaviourHandle(handle as u32))
}

/// Calls `call` with the spec's type name and its field values as JSON, or
/// null for none.
pub(crate) fn call_with_fields(spec: &BehaviourSpec, call: impl FnOnce(*const u8, i32, *const u8, i32) -> i32) -> i32 {
    let fields = if spec.fields.is_empty() { None } else { serde_json::to_string(&*spec.fields).ok() };
    let (fields_ptr, fields_len) = fields.as_deref().map_or((ptr::null(), 0), |f| (f.as_ptr(), f.len() as i32));
    call(spec.type_name.as_ptr(), spec.type_name.len() as i32, fields_ptr, fields_len)
}

/// A world-space placement made relative to `parent`, as of now. Like
/// Unity, only position and rotation are converted.
fn to_local(world: &World, parent: Entity, placement: Placement) -> Placement {
    let parent = world_matrix(world, parent);
    let (_, parent_rotation, _) = parent.to_scale_rotation_translation();
    Placement {
        position: parent.inverse().transform_point3(placement.position),
        rotation: (parent_rotation.inverse() * placement.rotation).normalize(),
    }
}

pub(crate) fn native_api(time: *const Time) -> NativeApi {
    NativeApi {
        size: size_of::<NativeApi>() as u32,
        version: API_VERSION,
        time,
        log,
        obj_name,
        transform_get,
        transform_set,
        behaviour_set_enabled,
        behaviour_destroy,
        level_load,
        obj_destroy,
        obj_behaviours,
        behaviour_add,
        prefab_load,
        prefab_instantiate,
        batch_handles,
        behaviours_adopt,
        obj_world_transform,
        load_async,
        load_status,
        load_allow_activation,
        prefab_keep,
        content_unload_unused,
        content_usage,
        level_load_async,
        level_info,
        obj_find,
        profiler_marker,
        profiler_begin,
        profiler_end,
        profiler_enabled,
        preview_create,
        preview_state,
        preview_save,
        preview_free,
        world_id: CURRENT_WORLD.as_ptr(),
        component_register,
        query_chunks,
        chunk_written,
        entity_create,
        entity_destroy,
        entity_alive,
        component_has,
        component_get,
        component_set,
        component_add,
        component_remove,
        commands_defer,
        change_tick,
        debug_line,
        debug_box,
        debug_sphere,
        debug_text,
        profiler_end_alloc,
        ui_load,
        ui_find,
        ui_close,
        ui_text,
        ui_set_text,
        ui_class,
        ui_visible,
        ui_clicked,
        ui_value,
        ui_style,
        screen_get: display::screen_get,
        screen_set: display::screen_set,
        cursor_get: display::cursor_get,
        cursor_set: display::cursor_set,
        scene_load_async,
        scene_unload,
        scene_set_active,
        ui_new,
        ui_create,
        ui_remove,
        ui_changed,
        save_write,
        save_wait,
        save_read,
        save_change,
        save_info,
        save_key,
        save_obj,
        save_scene,
        sound_load,
        sound_info,
        sound_play,
        voice_value,
        source_value,
        audio_bus,
        ui_items,
        input_action: crate::input::input_action,
        input_state: crate::input::input_state,
        input_control: crate::input::input_control,
        input_text: crate::input::input_text,
        input_change: crate::input::input_change,
        physics_value: crate::physics::physics_value,
        physics_push: crate::physics::physics_push,
        physics_raycast: crate::physics::physics_raycast,
        physics_overlap: crate::physics::physics_overlap,
        character_move: crate::physics::character_move,
        joint_connected: crate::physics::joint_connected,
        obj_layer: crate::layers::obj_layer,
        layer_name: crate::layers::layer_name,
        layer_find: crate::layers::layer_find,
        layer_collide: crate::layers::layer_collide,
        text_get: crate::text::text_get,
        text_language: crate::text::text_language,
        text_languages: crate::text::text_languages,
        text_version: crate::text::text_version,
        ui_keymap: crate::ui::ui_keymap,
        ui_keymap_clicked: crate::ui::ui_keymap_clicked,
        input_uses: crate::input::input_uses,
        input_label: crate::input::input_label,
        input_conflicts: crate::input::input_conflicts,
        text_asset: crate::text::text_asset,
        ui_set_string: crate::ui::ui_set_string,
        lighting_apply,
        seasons_time,
        seasons_set_time,
        water_sample,
        water_features,
        water_set_features,
        light_value,
        debug_screen_text,
        debug_solid,
        particle_value,
        smoke_act,
        smoke_value,
        obj_visible: crate::layers::obj_visible,
        mist_value,
        motion_matcher_value,
        bone_attachment_value,
        net_call,
        motion_matcher_text,
        net_object_value,
        net_fields,
        net_rpc,
        net_raycast,
        kind_value,
        kind_text,
        pose_op,
        asset_fields: crate::services::asset_fields,
        terrain_count: crate::terrain::terrain_count,
        terrain_ground: crate::terrain::terrain_ground,
        terrain_raycast: crate::terrain::terrain_raycast,
        obj_switch: crate::switches::obj_switch,
        obj_instantiate,
        plant_info: crate::plants::plant_info,
        plant_remove: crate::plants::plant_remove,
        plant_cut: crate::plants::plant_cut,
        plants_near: crate::plants::plants_near,
        terrain_edit: crate::terrain::terrain_edit,
        terrain_edits: crate::terrain::terrain_edits,
        ui_style_property: crate::ui::ui_style_property,
        data_assets: crate::services::data_assets,
        block_at: crate::blocks::block_at,
        block_set: crate::blocks::block_set,
        block_raycast: crate::blocks::block_raycast,
        block_edits: crate::blocks::block_edits,
        physics_gravity_at: crate::physics::physics_gravity_at,
        world_origin,
        block_info: crate::services::block_info,
        block_of: crate::blocks::block_of,
        block_set_named: crate::blocks::block_set_named,
        blocks_version: crate::blocks::blocks_version,
        water_splash,
        caves_info: crate::services::caves_info,
        particle_blast,
        blocks_all: crate::blocks::blocks_all,
        world_budget: crate::blocks::world_budget,
        set_world_budget: crate::blocks::set_world_budget,
        mob_motion_value,
        block_mob_value,
        biome_info: crate::services::biome_info,
        biome_at: crate::services::biome_at,
        weather_state,
        weather_set,
        look_pack_wear,
        look_packs_info: crate::services::look_packs_info,
        heat_sample: crate::heat::heat_sample,
        heat_add: crate::heat::heat_add,
        clock_value: crate::heat::clock_value,
        storm_call: crate::storms::storm_call,
        storm_state: crate::storms::storm_state,
        texture_pack_wear,
        texture_packs_info: crate::services::texture_packs_info,
        texture_pack_starter: crate::services::texture_pack_starter,
    }
}

/// After the systems of a stage ran: what they left waiting applies.
pub(crate) fn systems_ran() {
    ecs::settle_commands();
}
