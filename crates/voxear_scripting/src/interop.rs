//! The function tables shared with `Voxear.Core`. Every struct here has a
//! mirror in `managed/Voxear.Core/Interop`; change both together and bump
//! [`API_VERSION`].

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ptr,
};

use voxear_core::time::Time;
use voxear_ecs::{Entity, World};
use voxear_scene::{
    Adopted, AttachedBehaviour, BehaviourHandle, BehaviourSpec, Behaviours, Callbacks, InstanceMarker, Name, Placement,
    Registration, Request, Transform, world_matrix,
};

use crate::services::{ContentStats, LoadProgress, SpawnedPrefab, with_services};

pub(crate) const API_VERSION: u32 = 8;

/// Engine functions scripts call.
#[repr(C)]
pub(crate) struct NativeApi {
    pub size: u32,
    pub version: u32,
    pub time: *const Time,
    pub log: unsafe extern "C" fn(level: i32, message: *const u8, len: i32),
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
}

/// `behaviours_adopt` state bits, set by `Activation` in Voxear.Core.
const ADOPT_ENABLED: u32 = 1;
const ADOPT_WANTS_ENABLED: u32 = 2;
const ADOPT_DESTROYED: u32 = 4;

/// Type name, entity, field values as JSON (or null), and the info to fill.
pub(crate) type CreateBehaviour = unsafe extern "C" fn(*const u8, i32, u64, *const u8, i32, *mut BehaviourInfo) -> i32;

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
}

#[repr(C)]
#[derive(Default, Debug)]
pub(crate) struct BehaviourInfo {
    pub callbacks: u32,
    pub order: i32,
    pub flags: u32,
}

pub(crate) const FLAG_INSTANCE: u32 = 1;

thread_local! {
    static WORLD: Cell<*mut World> = const { Cell::new(ptr::null_mut()) };
    static REQUESTS: RefCell<Vec<Request>> = const { RefCell::new(Vec::new()) };
    static LEVEL_REQUESTS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static OBJ_DESTROYS: RefCell<Vec<Entity>> = const { RefCell::new(Vec::new()) };
    /// Managed `CreateBehaviour`, for behaviours created during script calls.
    static CREATE: Cell<Option<CreateBehaviour>> = const { Cell::new(None) };
    /// Behaviours created during script calls that scripts are still
    /// activating, until `behaviours_adopt` hands them to the lifecycle.
    static ACTIVATING: RefCell<HashMap<u32, Registration>> = RefCell::new(HashMap::new());
    /// The behaviours the last `prefab_instantiate` created, for `batch_handles`.
    static LAST_BATCH: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn set_create_behaviour(create: CreateBehaviour) {
    CREATE.set(Some(create));
}

pub(crate) fn take_obj_destroys() -> Vec<Entity> {
    OBJ_DESTROYS.take()
}

/// Lends the world to native callbacks for the duration of a script call.
pub(crate) struct WorldScope(*mut World);

impl WorldScope {
    pub fn enter(world: &mut World) -> Self {
        Self(WORLD.replace(world))
    }
}

impl Drop for WorldScope {
    fn drop(&mut self) {
        WORLD.set(self.0);
    }
}

pub(crate) fn drain_requests(out: &mut Vec<Request>) {
    REQUESTS.with_borrow_mut(|requests| out.append(requests));
}

pub(crate) fn take_level_requests() -> Vec<String> {
    LEVEL_REQUESTS.take()
}

fn with_world<R>(f: impl FnOnce(&mut World) -> Option<R>) -> Option<R> {
    let world = WORLD.get();
    if world.is_null() {
        return None;
    }
    f(unsafe { &mut *world })
}

unsafe fn utf8<'a>(ptr: *const u8, len: i32) -> std::borrow::Cow<'a, str> {
    if ptr.is_null() || len <= 0 {
        return "".into();
    }
    String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(ptr, len as usize) })
}

unsafe extern "C" fn log(level: i32, message: *const u8, len: i32) {
    let text = unsafe { utf8(message, len) };
    match level {
        0 => tracing::debug!(target: "script", "{text}"),
        1 => tracing::info!(target: "script", "{text}"),
        2 => tracing::warn!(target: "script", "{text}"),
        _ => tracing::error!(target: "script", "{text}"),
    }
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
            unsafe { out.write(transform) };
            1
        }
        None => 0,
    }
}

unsafe extern "C" fn transform_set(entity: u64, value: *const Transform) -> i32 {
    let value = unsafe { value.read() };
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

unsafe extern "C" fn load_status(id: u32, out: *mut LoadProgress) -> i32 {
    match with_services(|services| services.load_status(id)).flatten() {
        Some(status) => {
            unsafe { out.write(status) };
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
            unsafe { out.write(stats) };
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
    unsafe { out.write(Transform::new(position, rotation, scale)) };
    1
}

unsafe extern "C" fn obj_destroy(entity: u64) {
    OBJ_DESTROYS.with_borrow_mut(|d| d.push(Entity::from_bits(entity)));
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
    // Generation zero is the null entity, which is Obj.None.
    let parent = (parent >> 32 != 0).then(|| Entity::from_bits(parent));
    let placement = (!placement.is_null()).then(|| {
        let transform = unsafe { placement.read() };
        Placement { position: transform.position, rotation: transform.rotation }
    });
    let spawned = with_services(|services| {
        with_world(|world| {
            let placement = match (placement, parent) {
                (Some(placement), Some(parent)) if world_space != 0 => Some(to_local(world, parent, placement)),
                (placement, _) => placement,
            };
            Some(services.spawn_prefab(world, prefab as u32, parent, placement))
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
            tracing::error!(target: "script", "Instantiate: prefabs can only be instantiated while scripts run");
            return -1;
        }
    };
    // Behaviours are created only now, outside the world borrow, because
    // creating one runs managed code that may call back into the world.
    let handles: Vec<u32> = objs.iter().flat_map(|(entity, specs)| attach(*entity, specs)).collect();
    let count = handles.len() as i32;
    LAST_BATCH.set(handles);
    unsafe { out_root.write(root.to_bits()) };
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

/// Creates behaviours on an Obj during a script call. Scripts then run their
/// Awake and OnEnable and hand them back through `behaviours_adopt`.
fn attach(entity: Entity, specs: &[BehaviourSpec]) -> Vec<u32> {
    let Some(create) = CREATE.get() else { return Vec::new() };
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
        let registration =
            Registration { handle, entity, callbacks: Callbacks(info.callbacks), order: info.order, enabled: true };
        ACTIVATING.with_borrow_mut(|activating| activating.insert(handle.0, registration));
        is_instance |= info.flags & FLAG_INSTANCE != 0;
        attached.push(AttachedBehaviour { handle, spec: spec.clone() });
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
    let fields = if spec.fields.is_empty() { None } else { serde_json::to_string(&*spec.fields).ok() };
    let (fields_ptr, fields_len) = fields.as_deref().map_or((ptr::null(), 0), |f| (f.as_ptr(), f.len() as i32));
    let name = &spec.type_name;
    let handle = unsafe { create(name.as_ptr(), name.len() as i32, entity.to_bits(), fields_ptr, fields_len, info) };
    (handle >= 0).then_some(BehaviourHandle(handle as u32))
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
    }
}
