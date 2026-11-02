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
