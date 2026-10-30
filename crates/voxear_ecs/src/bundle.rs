use std::mem::ManuallyDrop;

use crate::component::{Component, ComponentId, ComponentRegistry};

/// A set of components inserted together: a single component or a tuple.
///
/// # Safety
/// `write` must hand out exactly the components listed by `component_ids`, in
/// the same order, and give up ownership of each one.
pub unsafe trait Bundle: Send + Sync + 'static {
    fn component_ids(registry: &mut ComponentRegistry, out: &mut Vec<ComponentId>);

    /// Moves each component out; `sink` receives its index and a pointer it
    /// must copy from (taking ownership).
    ///
    /// # Safety
    /// `sink` must take ownership of every value it is given.
    unsafe fn write(self, sink: &mut dyn FnMut(usize, *mut u8));
}

unsafe impl<T: Component> Bundle for T {
    fn component_ids(registry: &mut ComponentRegistry, out: &mut Vec<ComponentId>) {
        out.push(registry.register::<T>());
    }

    unsafe fn write(self, sink: &mut dyn FnMut(usize, *mut u8)) {
        let mut value = ManuallyDrop::new(self);
        sink(0, (&raw mut *value).cast());
    }
}

unsafe impl Bundle for () {
    fn component_ids(_: &mut ComponentRegistry, _: &mut Vec<ComponentId>) {}

    unsafe fn write(self, _: &mut dyn FnMut(usize, *mut u8)) {}
}

macro_rules! impl_bundle_tuple {
    ($($name:ident),+) => {
        #[allow(non_snake_case, unused_assignments)]
        unsafe impl<$($name: Component),+> Bundle for ($($name,)+) {
            fn component_ids(registry: &mut ComponentRegistry, out: &mut Vec<ComponentId>) {
                $(out.push(registry.register::<$name>());)+
            }

            unsafe fn write(self, sink: &mut dyn FnMut(usize, *mut u8)) {
                let ($($name,)+) = self;
                let mut index = 0;
                $(
                    let mut value = ManuallyDrop::new($name);
                    sink(index, (&raw mut *value).cast());
                    index += 1;
                )+
            }
        }
    };
}

impl_bundle_tuple!(A);
impl_bundle_tuple!(A, B);
impl_bundle_tuple!(A, B, C);
impl_bundle_tuple!(A, B, C, D);
impl_bundle_tuple!(A, B, C, D, E);
impl_bundle_tuple!(A, B, C, D, E, F);
impl_bundle_tuple!(A, B, C, D, E, F, G);
impl_bundle_tuple!(A, B, C, D, E, F, G, H);
impl_bundle_tuple!(A, B, C, D, E, F, G, H, I);
impl_bundle_tuple!(A, B, C, D, E, F, G, H, I, J);
impl_bundle_tuple!(A, B, C, D, E, F, G, H, I, J, K);
impl_bundle_tuple!(A, B, C, D, E, F, G, H, I, J, K, L);
