use std::{
    alloc::Layout,
    any::{TypeId, type_name},
    borrow::Cow,
    collections::HashMap,
};

/// Marks a Rust type as storable in chunks. Components are plain data; logic
/// lives in systems.
pub trait Component: Send + Sync + 'static {}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
#[repr(transparent)]
pub struct ComponentId(pub(crate) u32);

impl ComponentId {
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// The id with this index in a world's registry, as scripts hand it
    /// back; check it against [`ComponentRegistry::len`] before use.
    pub const fn from_index(index: u32) -> Self {
        Self(index)
    }
}

pub(crate) type DropFn = unsafe fn(*mut u8);

#[derive(Clone, Debug)]
pub struct ComponentInfo {
    name: Cow<'static, str>,
    layout: Layout,
    drop: Option<DropFn>,
    type_id: Option<TypeId>,
}

impl ComponentInfo {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn layout(&self) -> Layout {
        self.layout
    }

    /// `None` for runtime-registered components, which have no Rust type.
    pub fn type_id(&self) -> Option<TypeId> {
        self.type_id
    }

    pub(crate) fn drop_fn(&self) -> Option<DropFn> {
        self.drop
    }

    /// Whether the value owns something, as a `String` does, so its bytes
    /// cannot be copied or read as plain data.
    pub fn needs_drop(&self) -> bool {
        self.drop.is_some()
    }
}

unsafe fn drop_ptr<T>(ptr: *mut u8) {
    unsafe { ptr.cast::<T>().drop_in_place() }
}

#[derive(Default)]
pub struct ComponentRegistry {
    infos: Vec<ComponentInfo>,
    by_type: HashMap<TypeId, ComponentId>,
    by_name: HashMap<Cow<'static, str>, ComponentId>,
}

impl ComponentRegistry {
    pub fn register<T: Component>(&mut self) -> ComponentId {
        if let Some(&id) = self.by_type.get(&TypeId::of::<T>()) {
            return id;
        }
        let id = self.push(ComponentInfo {
            name: Cow::Borrowed(type_name::<T>()),
            layout: Layout::new::<T>(),
            drop: std::mem::needs_drop::<T>().then_some(drop_ptr::<T> as DropFn),
            type_id: Some(TypeId::of::<T>()),
        });
        self.by_type.insert(TypeId::of::<T>(), id);
        id
    }

    /// Registers a Rust component under a stable name so scripts can find it.
    pub fn register_named<T: Component>(&mut self, name: impl Into<Cow<'static, str>>) -> ComponentId {
        let id = self.register::<T>();
        let name = name.into();
        if let Some(&existing) = self.by_name.get(&name) {
            assert_eq!(existing, id, "component name `{name}` is already used by another component");
            return id;
        }
        self.infos[id.index()].name = name.clone();
        self.by_name.insert(name, id);
        id
    }

    /// Registers a plain-old-data component that has no Rust type, such as a
    /// C# `unmanaged` struct. Registering the same name twice returns the same
    /// id, and panics if the layout changed.
    pub fn register_dynamic(&mut self, name: impl Into<Cow<'static, str>>, layout: Layout) -> ComponentId {
        let name = name.into();
        if let Some(&id) = self.by_name.get(&name) {
            let existing = self.infos[id.index()].layout;
            assert_eq!(existing, layout, "component `{name}` re-registered with a different layout");
            return id;
        }
        let id = self.push(ComponentInfo { name: name.clone(), layout, drop: None, type_id: None });
        self.by_name.insert(name, id);
        id
    }

    pub fn id_of<T: Component>(&self) -> Option<ComponentId> {
        self.by_type.get(&TypeId::of::<T>()).copied()
    }

    pub fn id_by_name(&self, name: &str) -> Option<ComponentId> {
        self.by_name.get(name).copied()
    }

    pub fn info(&self, id: ComponentId) -> &ComponentInfo {
        &self.infos[id.index()]
    }

    pub fn len(&self) -> usize {
        self.infos.len()
    }

    pub fn is_empty(&self) -> bool {
        self.infos.is_empty()
    }

    fn push(&mut self, info: ComponentInfo) -> ComponentId {
        let id = ComponentId(u32::try_from(self.infos.len()).expect("too many component types"));
        self.infos.push(info);
        id
    }
}
