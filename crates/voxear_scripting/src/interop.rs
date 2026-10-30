//! The function tables shared with `Voxear.Core`. Every struct here has a
//! mirror in `managed/Voxear.Core/Interop`; change both together and bump
//! [`API_VERSION`].

use std::{
    cell::{Cell, RefCell},
    ptr,
};

use voxear_core::time::Time;
use voxear_ecs::{Entity, World};
use voxear_scene::{BehaviourHandle, Name, Request, Transform};

pub(crate) const API_VERSION: u32 = 2;

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
}

/// Script functions the engine calls; filled in by `Bootstrap.Initialize`.
#[repr(C)]
#[derive(Default)]
pub(crate) struct ManagedApi {
    pub size: u32,
    pub version: u32,
    pub load_game_assembly: Option<unsafe extern "C" fn(path: *const u8, len: i32) -> i32>,
    pub create_behaviour:
        Option<unsafe extern "C" fn(type_name: *const u8, len: i32, entity: u64, info: *mut BehaviourInfo) -> i32>,
    pub invoke: Option<unsafe extern "C" fn(phase: i32, handles: *const u32, count: i32)>,
    pub release: Option<unsafe extern "C" fn(handles: *const u32, count: i32)>,
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
    }
}
