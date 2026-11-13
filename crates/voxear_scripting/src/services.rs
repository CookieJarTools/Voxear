//! Engine services that native callbacks reach while scripts run, beyond
//! the world: loading and instantiating prefabs. The engine implements
//! [`ScriptServices`] over its content and lends it for each frame.

use std::{cell::Cell, marker::PhantomData, ptr::NonNull};

use anyhow::Result;
use voxear_ecs::{Entity, World};
use voxear_scene::{BehaviourSpec, Placement};

use crate::{
    audio::{AudioAsk, PlaySound, SoundFacts},
    display::CursorLock,
};

/// What scripts reach beyond the world. Each part has a default that says
/// there is none of it here, so a host lends only what it has, as the
/// Inspector lends only its UI to the drawers scripts draw fields with.
pub trait ScriptServices {
    /// Loads a prefab by GUID or project path. Loading it again returns the
    /// same id.
    fn load_prefab(&mut self, reference: &str) -> Result<u32> {
        anyhow::bail!("{reference}: there are no prefabs to load here")
    }

    /// Creates a loaded prefab's Objs, without behaviours. The root goes
    /// under `parent`, and takes `placement`, relative to it, when it is
    /// given.
    fn spawn_prefab(
        &mut self,
        _world: &mut World,
        _prefab: u32,
        _parent: Option<Entity>,
        _placement: Option<Placement>,
    ) -> Result<SpawnedPrefab> {
        anyhow::bail!("there are no prefabs to make here")
    }

    /// Copies an Obj and every Obj under it, without behaviours, as
    /// [`spawn_prefab`](Self::spawn_prefab) makes a prefab's: the copy's
    /// behaviours are the original's as its scene or prefab gave them.
    fn copy_obj(
        &mut self,
        _world: &mut World,
        _original: Entity,
        _parent: Option<Entity>,
        _placement: Option<Placement>,
    ) -> Result<SpawnedPrefab> {
        anyhow::bail!("there are no Objs to copy here")
    }

    /// Starts loading a level in the background that changes over as
    /// `options` say.
    fn load_level(&mut self, reference: &str, _options: LevelLoadOptions) -> Result<u32> {
        anyhow::bail!("{reference}: there are no levels to load here")
    }

    /// Starts loading one scene in the background, to come in alongside
    /// those loaded as `options` say.
    fn load_scene(&mut self, reference: &str, _options: SceneLoadOptions) -> Result<u32> {
        anyhow::bail!("{reference}: there are no scenes to load here")
    }

    /// What scripts can ask about levels: see [`LevelInfo`]. `id` names a
    /// background load for the kinds that are about one.
    fn level_info(&self, _what: LevelInfo, _id: u32) -> Option<String> {
        None
    }

    /// Starts loading a level (`level` true) or a prefab in the background.
    fn load_async(&mut self, _level: bool, reference: &str) -> Result<u32> {
        anyhow::bail!("{reference}: there is nothing to load here")
    }

    fn load_status(&self, _id: u32) -> Option<LoadProgress> {
        None
    }

    fn allow_activation(&mut self, _id: u32, _allow: bool) {}

    /// Keeps a loaded prefab across sweeps, or lets it go again.
    fn keep_prefab(&mut self, _prefab: u32, _keep: bool) {}

    /// The game's window as whatever shows the game has it: its size in
    /// pixels, and whether it fills the screen.
    fn screen(&self) -> (u32, u32, bool) {
        (0, 0, false)
    }

    /// A size and full screen scripts ask the game's window for.
    fn set_resolution(&mut self, _width: u32, _height: u32, _fullscreen: bool) {}

    /// The game's input, for `Input`, `Keyboard`, `Mouse` and `Gamepad`.
    fn input(&mut self) -> Option<&mut voxear_input::Input> {
        None
    }

    /// Whether the mouse shows, and how it is held.
    fn cursor(&self) -> (bool, CursorLock) {
        (true, CursorLock::None)
    }

    /// Shows or hides the mouse, and holds it, as scripts ask.
    fn set_cursor(&mut self, _visible: bool, _lock: CursorLock) {}

    /// The project path of the file a loaded prefab came from, as a log line
    /// about it names it.
    fn prefab_path(&self, _prefab: u32) -> Option<String> {
        None
    }

    /// Frees what nothing uses at the end of the frame.
    fn unload_unused(&mut self) {}

    fn usage(&self) -> ContentStats {
        ContentStats::default()
    }

    /// Asks for a preview of an asset, by GUID or project path, or of a
    /// loaded prefab, drawn as `options` say. It is drawn over the next
    /// frames.
    fn create_preview(&mut self, _source: PreviewSource<'_>, _options: &PreviewOptions) -> Result<u32> {
        anyhow::bail!("there are no previews here")
    }

    /// Where a preview is drawn, once it is; `None` for one that does not
    /// exist.
    fn preview_state(&mut self, _preview: u32) -> Option<PreviewState> {
        None
    }

    /// Writes a preview to a PNG file at `path` once it is drawn and read
    /// back, a few frames from now.
    fn save_preview(&mut self, _preview: u32, path: &str) -> Result<()> {
        anyhow::bail!("{path}: there are no previews here")
    }

    fn free_preview(&mut self, _preview: u32) {}

    /// The game's own UI (`Voxear.UiDocument`): a document loaded by project
    /// path, and its elements found by name, each a number scripts hold.
    /// Without a UI to show it in, there is none.
    fn ui_load(&mut self, path: &str) -> Result<u32> {
        anyhow::bail!("{path}: there is no UI to show it in here")
    }

    fn ui_find(&mut self, _document: u32, _name: &str) -> Option<u32> {
        None
    }

    fn ui_close(&mut self, _document: u32) {}

    /// An empty document, for scripts to fill with elements made from code.
    fn ui_new(&mut self) -> Result<u32> {
        anyhow::bail!("there is no UI to show a document in here")
    }

    /// Makes an element at the end of `parent`: `kind` is its number among
    /// `voxear_ui::ElementKind::ALL`, and `text` as `Ui::create` reads it.
    fn ui_create(&mut self, _parent: u32, _kind: i32, _text: &str) -> Option<u32> {
        None
    }

    /// Takes an element out, with everything in it.
    fn ui_remove(&mut self, _element: u32) {}

    /// Something asked of an element: see [`UiAsk`]. The answer is what was
    /// read, or what is true after a change.
    fn ui_ask(&mut self, _element: u32, _ask: UiAsk<'_>) -> UiAnswer {
        UiAnswer::None
    }

    /// The number scripts hold a UI Panel's document by, loaded now if it
    /// has not been yet, so a script finds it in Awake and Start; none while
    /// the panel shows no document, or here, where there is no UI.
    fn ui_panel_document(&mut self, _world: &mut World, _entity: Entity) -> Option<u32> {
        None
    }

    /// The same for a Screen UI's document.
    fn ui_screen_document(&mut self, _world: &mut World, _entity: Entity) -> Option<u32> {
        None
    }

    /// What a save calls an Obj, so that a later run finds it again: the
    /// GUID of the scene it was loaded from and its path there, as
    /// `guid:3/2`. An Obj a script made from a prefab is `spawned:` and its
    /// copy's root entity, a slash and its place in the prefab, which holds
    /// only for this run; a save of the scene numbers the copies itself.
    /// None for any other Obj.
    fn obj_key(&self, _world: &World, _entity: Entity) -> Option<String> {
        None
    }

    /// The Obj a key names, when its scene is open or its copy exists.
    fn obj_by_key(&self, _world: &World, _key: &str) -> Option<Entity> {
        None
    }

    /// The open scenes' state as JSON, for a save of the scene: the level's
    /// GUID, the keys of the Objs scenes loaded that are gone, and the root
    /// of each copy scripts made from a prefab, with the prefab's GUID and
    /// the key of its parent (see [`obj_key`](Self::obj_key)). `doomed` are
    /// the Objs scripts asked to destroy that are still waiting, which count
    /// as gone, with everything under them.
    fn scene_state(&self, _world: &World, _doomed: &[Entity]) -> Option<String> {
        None
    }

    /// An asset's GUID, as a save keeps it.
    fn asset_guid(&self, _asset: AssetName<'_>) -> Option<String> {
        None
    }

    /// The number of a terrain's surface drawn with `material`, a project
    /// path or GUID, in its asset's list; none where it has no such surface.
    fn terrain_surface(&self, _world: &World, _terrain: Entity, _material: &str) -> Option<u8> {
        None
    }

    /// A data asset (an `.asset` file) by project path or GUID, for
    /// `Definition.Load`, as JSON: its GUID, its path, its type and its fields.
    fn asset_fields(&mut self, reference: &str) -> Result<String> {
        anyhow::bail!("data assets load only in a running game, not here ({reference})")
    }

    /// What a block (a `.block` file) says, by project path or GUID, for
    /// `Block` in Voxear.Core, as JSON: its GUID, its name, its look and its
    /// rules.
    fn block_json(&mut self, reference: &str) -> Result<String> {
        anyhow::bail!("blocks are read only in a running game, not here ({reference})")
    }

    /// Goes up whenever a block is saved, so what scripts read of one is
    /// read again.
    fn blocks_version(&mut self) -> u64 {
        0
    }

    /// Every block of the project, by its GUID, in the order of their
    /// paths, for `Blocks.All` in Voxear.Core.
    fn block_guids(&mut self) -> Vec<String> {
        Vec::new()
    }

    /// What a biome (a `.biome` file) says, by project path or GUID, for
    /// `Biome` in Voxear.Core, as JSON: its GUID, its name and its colour.
    fn biome_json(&mut self, reference: &str) -> Result<String> {
        anyhow::bail!("biomes are read only in a running game, not here ({reference})")
    }

    /// Every look pack there is to wear, for `LookPacks` in Voxear.Core, as
    /// JSON: `packs`, each with what names it, its name, its description
    /// and where it is from; `worn`, what names the one worn, if any; and
    /// `folder`, where a player keeps their own.
    fn look_packs_json(&mut self, _world: &World) -> Result<String> {
        anyhow::bail!("look packs are listed only in a running game, not here")
    }

    /// Every texture pack there is to wear, for `TexturePacks` in
    /// Voxear.Core, as JSON: `packs`, each with what names it, its name,
    /// its description and where it is from; `worn`, what names the one
    /// worn, if any; and `folder`, where a player keeps their own.
    fn texture_packs_json(&mut self) -> Result<String> {
        anyhow::bail!("texture packs are listed only in a running game, not here")
    }

    /// Makes a texture pack to start from, called `name`, in the player's
    /// folder, for `TexturePacks.WriteStarter`: what names it.
    fn texture_pack_starter(&mut self, name: &str) -> Result<String> {
        anyhow::bail!("a texture pack is started only in a running game, not here ({name})")
    }

    /// The biome the place `at` is, by its GUID, for `Biomes.At` in
    /// Voxear.Core: none where no world of blocks with biomes reaches.
    fn biome_at(&mut self, _world: &World, _at: [f32; 3]) -> Option<String> {
        None
    }

    /// The cave rooms of the world of blocks on `obj` (the first with any,
    /// for 0), for `Caves` in Voxear.Core, as JSON: the world's Obj, its
    /// network's layout, and rooms as they lie in the level. `what` picks
    /// which rooms, by `at`: 0 for a level's own, in order; 1 for those
    /// within `at[3]` metres of the place `at`; 2 for the one that place is
    /// inside; 3 for the one of the region whose three numbers `at` holds;
    /// 4 for none, the network alone.
    fn caves_json(&mut self, _world: &World, obj: u64, _what: i32, _at: [f32; 4]) -> Result<String> {
        anyhow::bail!("caves are read only in a running game, not here (Obj {obj})")
    }

    /// The GUID of the block numbered `block` in the world of blocks on
    /// `entity`: none for air, or a kind the world keeps itself.
    fn block_guid(&mut self, _world: &World, _entity: Entity, _block: u16) -> Option<String> {
        None
    }

    /// The number of the block `reference` names in the world of blocks on
    /// `entity`: the one it has, or, with `grow`, a number given it now
    /// where the world has not held it yet, its faces given to the world.
    /// None where there is no such block, or no room for it.
    fn block_number(&mut self, _world: &mut World, _entity: Entity, _reference: &str, _grow: bool) -> Option<u16> {
        None
    }

    /// As [`block_number`](Self::block_number), the block placed in
    /// `state` (see `voxear_terrain::blocks::shape::BlockState`): each state
    /// of a block a world holds is a number of its own.
    fn block_number_state(
        &mut self,
        world: &mut World,
        entity: Entity,
        reference: &str,
        _state: u8,
        grow: bool,
    ) -> Option<u16> {
        self.block_number(world, entity, reference, grow)
    }

    /// Uses the block at `at`, in blocks from the corner of the world of
    /// blocks on `entity` (see `voxear_terrain::blocks::usable`): what kind
    /// of use it was, by `UseKind`'s place, and whether it is open (or on)
    /// now. None where it is no block a player uses.
    fn block_use(&mut self, _world: &mut World, _entity: Entity, _at: [i32; 3]) -> Option<(u8, bool)> {
        None
    }

    /// Puts the block at `at` of the world of blocks on `entity` in
    /// `state`, the same block, moving there by its motion where `animate`
    /// says and only its open or on changes: a door with its pair, a lever
    /// with what it drives. How many blocks changed.
    fn block_change_state(&mut self, _world: &mut World, _entity: Entity, _at: [i32; 3], _state: u8, _animate: bool) -> usize {
        0
    }

    /// `changes` to the world of blocks on `entity` with what goes with
    /// each: a door's upper block over a door put down, and a door's other
    /// block taken away with it.
    fn door_halves(&mut self, _world: &mut World, _entity: Entity, changes: Vec<([i32; 3], u16)>) -> Vec<([i32; 3], u16)> {
        changes
    }

    /// The blocks the world of blocks on `entity` numbered since its file
    /// was read, whose numbers are this machine's own: each number, and its
    /// block's GUID.
    fn blocks_grown(&mut self, _world: &World, _entity: Entity) -> Vec<(u16, String)> {
        Vec::new()
    }

    /// Whether `block` is the number of a block in the world of blocks on
    /// `entity`.
    fn block_listed(&mut self, _world: &World, _entity: Entity, _block: u16) -> bool {
        true
    }

    /// Every data asset of the project, for `Definition.All`, as JSON: each
    /// one's GUID, its path and the type it names, by path.
    fn data_assets(&mut self) -> Result<String> {
        anyhow::bail!("data assets are listed only in a running game, not here")
    }

    /// Loads a sound asset by project path or GUID, for `Sound.Load`,
    /// returning its id.
    fn load_sound(&mut self, reference: &str) -> Result<u32> {
        anyhow::bail!("sounds load only in a running game, not here ({reference})")
    }

    /// What a loaded sound holds.
    fn sound_info(&mut self, _sound: u32) -> Option<SoundFacts> {
        None
    }

    /// Plays a sound once, for `Audio.Play`, returning its voice, or 0.
    fn play_sound(&mut self, _sound: u32, _play: &PlaySound) -> u64 {
        0
    }

    /// Reads, and sets when `set` says, something of a voice, an Audio
    /// Source in `world`, or a bus (see [`AudioAsk`]); `None` where there
    /// is no such voice or source, or no sound here.
    fn audio(&mut self, _world: Option<&mut World>, _ask: AudioAsk, _set: Option<f32>) -> Option<f32> {
        None
    }
}

/// An asset a save names by GUID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetName<'a> {
    /// By project path or GUID, as a `Ref<T>` holds it.
    Reference(&'a str),
    /// A loaded prefab, by the id [`ScriptServices::load_prefab`] gave it.
    Prefab(u32),
    /// A loaded sound, by the id [`ScriptServices::load_sound`] gave it.
    Sound(u32),
}

/// What a script asks of one of the game UI's elements.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UiAsk<'a> {
    Text,
    SetText(&'a str),
    /// Adds (1), removes (0), toggles (2) or only looks for (3) a class.
    Class(&'a str, u8),
    Visible,
    SetVisible(bool),
    /// Whether it was clicked since the last frame.
    Clicked,
    /// Whether its value changed since the last frame: typed in, ticked,
    /// moved or picked.
    Changed,
    /// A slider's value, or a toggle's, 1 when ticked.
    Value,
    SetValue(f32),
    /// Declarations that win over its sheets.
    SetStyle(&'a str),
    /// One property of those, by its name and value, in place of what it
    /// had for it; an empty value takes it out.
    SetStyleProperty(&'a str, &'a str),
    /// What shows when the pointer rests on it; empty for nothing.
    SetTooltip(&'a str),
    /// What a text field shows, faded, while nothing is typed.
    SetPlaceholder(&'a str),
    /// A dropdown's options, one a line, keeping the one picked.
    SetOptions(&'a str),
    /// The picture an image shows, by project path or GUID; empty for none.
    /// Whatever has the pictures finds it and asks `SetImage`.
    SetPicture(&'a str),
    /// How a map view shows the ground: settings by their names, as
    /// `voxear_ui::MapViewSettings::set_all` reads them, as
    /// "zoom=4 turns=true fill=1.2".
    SetMapView(&'a str),
    /// What an image shows: the renderer's index for a texture and the part
    /// of it, as fractions (left, top, right, bottom); none for nothing.
    SetImage(Option<(u32, [f32; 4])>),
    /// A list view's items: how many, how tall each row is, and for a tree
    /// view, each item's id and its parent's, 0 at the top, in order.
    Items {
        count: usize,
        tree: Option<&'a [[u64; 2]]>,
        item_height: f32,
    },
    /// What a key map shows: the keyboard and mouse (0) or a gamepad (1),
    /// coloured by being assigned (0) or by category (1), whether hidden
    /// actions are labelled, and one map's actions, or every map's for "".
    Keymap {
        scheme: u8,
        colors: u8,
        hidden: bool,
        map: &'a str,
    },
    /// The path of the control clicked on a key map since the last frame.
    KeymapClicked,
}

/// What an element answered.
#[derive(Clone, Debug, PartialEq)]
pub enum UiAnswer {
    None,
    Text(String),
    Yes(bool),
    Number(f32),
}

/// What a script asks for a preview of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewSource<'a> {
    /// An asset by GUID or project path, or a mesh such as `builtin:cube`.
    Asset(&'a str),
    /// A loaded prefab, by the id [`ScriptServices::load_prefab`] gave it.
    Prefab(u32),
}

/// How a preview looks, as scripts ask for it. Mirrors `PreviewOptionsData`
/// in Voxear.Core, and the renderer's `PreviewOptions`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct PreviewOptions {
    pub size: u32,
    /// sRGB colour and alpha.
    pub background: [f32; 4],
    /// x, y, z, w.
    pub rotation: [f32; 4],
    pub offset: [f32; 2],
    pub zoom: f32,
    pub fov_degrees: f32,
    pub light_from: [f32; 3],
    pub light_color: [f32; 3],
    pub light_intensity: f32,
    pub ambient: f32,
    pub exposure: f32,
}

const _: () = assert!(size_of::<PreviewOptions>() == 88, "PreviewOptionsData in Voxear.Core is 88 bytes");

/// Where a preview is, as scripts see it. Mirrors `PreviewStateData` in
/// Voxear.Core.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct PreviewState {
    /// 1 once it is drawn.
    pub ready: u32,
    /// The renderer's texture holding it, and its rectangle there as
    /// fractions: left, top, right, bottom.
    pub texture: u32,
    pub region: [f32; 4],
}

const _: () = assert!(size_of::<PreviewState>() == 24, "PreviewStateData in Voxear.Core is 24 bytes");

/// What is loaded, as scripts see it. Mirrors `ContentUsage` in Voxear.Core.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct ContentStats {
    pub models: u32,
    pub prefabs: u32,
    pub kept_prefabs: u32,
    pub materials: u32,
    pub gpu_meshes: u32,
    pub gpu_materials: u32,
    pub gpu_textures: u32,
    /// Freed, waiting for frames in flight to finish with them.
    pub gpu_retiring: u32,
}

/// A background load as scripts see it.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct LoadProgress {
    /// 0 loading, 1 ready and waiting for activation, 2 done, 3 failed.
    pub state: u32,
    pub progress: f32,
    /// The loaded prefab once a prefab load is done, or -1.
    pub prefab: i32,
}

/// How a level change is made, as scripts ask for it. Mirrors
/// `LevelLoadOptions` in Voxear.Core.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct LevelLoadOptions {
    pub hold: i32,
    pub least_time: f32,
    pub unload_rest: i32,
    pub set_active: i32,
    pub tidy_up: i32,
    pub reload_shared: i32,
}

/// How a scene is loaded alongside those loaded, as scripts ask for it.
/// Mirrors `SceneLoadOptions` in Voxear.Core.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct SceneLoadOptions {
    /// Make it the active scene once it is in.
    pub set_active: i32,
    /// Load it again if it is loaded already.
    pub reload: i32,
    /// Wait at ready until activation is allowed, as a held level change does.
    pub hold: i32,
}

/// What scripts ask of a loaded scene, done at the end of the frame, as a
/// level change is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SceneRequest {
    Unload(String),
    SetActive(String),
}

/// What [`ScriptServices::level_info`] answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LevelInfo {
    /// The level loaded now.
    Current,
    /// The scene runtime Objs go into.
    ActiveScene,
    /// The scenes a finished change brought in, one per line.
    Scenes,
    /// Why a change failed.
    Error,
    /// Every scene loaded now, one per line.
    Loaded,
}

impl LevelInfo {
    pub fn from_code(code: i32) -> Option<Self> {
        Some(match code {
            0 => LevelInfo::Current,
            1 => LevelInfo::ActiveScene,
            2 => LevelInfo::Scenes,
            3 => LevelInfo::Error,
            4 => LevelInfo::Loaded,
            _ => return None,
        })
    }
}

/// What [`ScriptServices::spawn_prefab`] created.
pub struct SpawnedPrefab {
    pub root: Entity,
    /// Every new Obj, with the behaviours to create on it.
    pub objs: Vec<(Entity, Vec<BehaviourSpec>)>,
}

thread_local! {
    static SERVICES: Cell<Option<NonNull<dyn ScriptServices>>> = const { Cell::new(None) };
}

/// Lends services to native callbacks until it is dropped.
pub struct ServicesScope<'a> {
    previous: Option<NonNull<dyn ScriptServices>>,
    _borrow: PhantomData<&'a mut ()>,
}

impl<'a> ServicesScope<'a> {
    pub fn enter(services: &'a mut (dyn ScriptServices + 'a)) -> Self {
        let pointer = NonNull::from(services);
        // The scope's lifetime keeps the borrow alive for as long as the
        // pointer is reachable, which the type cannot say by itself.
        let pointer: NonNull<dyn ScriptServices> = unsafe { std::mem::transmute(pointer) };
        Self { previous: SERVICES.replace(Some(pointer)), _borrow: PhantomData }
    }
}

impl Drop for ServicesScope<'_> {
    fn drop(&mut self) {
        SERVICES.set(self.previous);
    }
}

/// Runs `f` with the lent services, or returns `None` outside any scope.
pub(crate) fn with_services<R>(f: impl FnOnce(&mut dyn ScriptServices) -> R) -> Option<R> {
    let mut services = SERVICES.get()?;
    Some(f(unsafe { services.as_mut() }))
}

/// A UI Panel's document through the lent services, which load it if it has
/// not loaded yet (see [`ScriptServices::ui_panel_document`]); none outside
/// them.
pub fn ui_panel_document(world: &mut World, entity: Entity) -> Option<u32> {
    with_services(|services| services.ui_panel_document(world, entity)).flatten()
}

/// A Screen UI's document through the lent services, as
/// [`ui_panel_document`] gives a UI Panel's.
pub fn ui_screen_document(world: &mut World, entity: Entity) -> Option<u32> {
    with_services(|services| services.ui_screen_document(world, entity)).flatten()
}

/// A data asset as JSON for `Definition.Load` (see
/// [`ScriptServices::asset_fields`]), written when it fits: its length, or
/// -1 when there is none.
pub(crate) unsafe extern "C" fn asset_fields(reference: *const u8, len: i32, buffer: *mut u8, capacity: i32) -> i32 {
    let reference = unsafe { crate::interop::utf8(reference, len) };
    unsafe { answer_json(with_services(|services| services.asset_fields(&reference)), buffer, capacity) }
}

/// A block as JSON for `Block` in Voxear.Core (see
/// [`ScriptServices::block_json`]), written when it fits: its length, or
/// -1 when there is none.
pub(crate) unsafe extern "C" fn block_info(reference: *const u8, len: i32, buffer: *mut u8, capacity: i32) -> i32 {
    let reference = unsafe { crate::interop::utf8(reference, len) };
    // No block by that name is an answer, not an error: `Block.Load` says so itself.
    let said = with_services(|services| services.block_json(&reference).ok());
    let Some(Some(json)) = said else { return -1 };
    if !buffer.is_null() && json.len() <= capacity.max(0) as usize {
        unsafe { std::ptr::copy_nonoverlapping(json.as_ptr(), buffer, json.len()) };
    }
    json.len() as i32
}

/// A biome as JSON for `Biome` in Voxear.Core (see
/// [`ScriptServices::biome_json`]), written when it fits: its length, or
/// -1 when there is none.
pub(crate) unsafe extern "C" fn biome_info(reference: *const u8, len: i32, buffer: *mut u8, capacity: i32) -> i32 {
    let reference = unsafe { crate::interop::utf8(reference, len) };
    // No biome by that name is an answer, not an error: `Biome.Load` says so itself.
    let said = with_services(|services| services.biome_json(&reference).ok());
    let Some(Some(json)) = said else { return -1 };
    if !buffer.is_null() && json.len() <= capacity.max(0) as usize {
        unsafe { std::ptr::copy_nonoverlapping(json.as_ptr(), buffer, json.len()) };
    }
    json.len() as i32
}

/// The GUID of the biome the place `at` is (see
/// [`ScriptServices::biome_at`]), written into `buffer` when it fits: its
/// length, or 0 where no world of blocks with biomes reaches.
pub(crate) unsafe extern "C" fn biome_at(at: *const [f32; 3], buffer: *mut u8, capacity: i32) -> i32 {
    if at.is_null() {
        return 0;
    }
    let at = unsafe { *at };
    let found = crate::interop::with_world(|world| with_services(|services| services.biome_at(world, at)).flatten());
    let Some(guid) = found else { return 0 };
    if !buffer.is_null() && guid.len() <= capacity.max(0) as usize {
        unsafe { std::ptr::copy_nonoverlapping(guid.as_ptr(), buffer, guid.len()) };
    }
    guid.len() as i32
}

/// The cave rooms of a world of blocks as JSON for `Caves` in Voxear.Core
/// (see [`ScriptServices::caves_json`]), written when it fits: its length,
/// or -1 where there are none.
pub(crate) unsafe extern "C" fn caves_info(
    obj: u64,
    what: i32,
    at: *const [f32; 4],
    buffer: *mut u8,
    capacity: i32,
) -> i32 {
    let at = if at.is_null() { [0.0; 4] } else { unsafe { *at } };
    // A level with no caves is an answer, not an error: `Caves.Rooms` is empty there.
    let said = crate::interop::with_world(|world| {
        with_services(|services| services.caves_json(world, obj, what, at).ok())
    });
    let Some(Some(json)) = said else { return -1 };
    if !buffer.is_null() && json.len() <= capacity.max(0) as usize {
        unsafe { std::ptr::copy_nonoverlapping(json.as_ptr(), buffer, json.len()) };
    }
    json.len() as i32
}

/// Every data asset of the project as JSON for `Definition.All` (see
/// [`ScriptServices::data_assets`]), written when it fits: its length, or
/// -1 when they cannot be listed here.
pub(crate) unsafe extern "C" fn data_assets(buffer: *mut u8, capacity: i32) -> i32 {
    unsafe { answer_json(with_services(|services| services.data_assets()), buffer, capacity) }
}

/// The look packs as JSON for `LookPacks` in Voxear.Core (see
/// [`ScriptServices::look_packs_json`]), written when it fits: its length,
/// or -1 when there is none.
pub(crate) unsafe extern "C" fn look_packs_info(buffer: *mut u8, capacity: i32) -> i32 {
    let said = crate::interop::with_world(|world| with_services(|services| services.look_packs_json(world)));
    unsafe { answer_json(said, buffer, capacity) }
}

/// The texture packs as JSON for `TexturePacks` in Voxear.Core (see
/// [`ScriptServices::texture_packs_json`]), written when it fits: its
/// length, or -1 when there is none.
pub(crate) unsafe extern "C" fn texture_packs_info(buffer: *mut u8, capacity: i32) -> i32 {
    unsafe { answer_json(with_services(|services| services.texture_packs_json()), buffer, capacity) }
}

/// What names the texture pack made to start from (see
/// [`ScriptServices::texture_pack_starter`]), written when it fits: its
/// length, or -1, with the error logged, where it could not be made.
pub(crate) unsafe extern "C" fn texture_pack_starter(name: *const u8, len: i32, buffer: *mut u8, capacity: i32) -> i32 {
    let name = unsafe { crate::interop::utf8(name, len) };
    unsafe { answer_json(with_services(|services| services.texture_pack_starter(&name)), buffer, capacity) }
}

/// Writes JSON a service gave into `buffer` when it fits, and gives its
/// length; -1, with the error logged, for none.
unsafe fn answer_json(answer: Option<Result<String>>, buffer: *mut u8, capacity: i32) -> i32 {
    match answer {
        Some(Ok(json)) => {
            if !buffer.is_null() && json.len() <= capacity.max(0) as usize {
                unsafe { std::ptr::copy_nonoverlapping(json.as_ptr(), buffer, json.len()) };
            }
            json.len() as i32
        }
        Some(Err(error)) => {
            tracing::error!("{error:#}");
            -1
        }
        None => -1,
    }
}
