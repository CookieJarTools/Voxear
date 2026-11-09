//! The Guns module: guns held in a character's own hands, every motion of
//! them made as it happens, or for a gun from an animation pack, by its
//! clips (see [`clip_hold`] and [`gun_clips`]). A Gun is data on a gun's
//! model: its kind, weight and sockets. A Gun Holder holds one in front of
//! its First-Person View, and aims, sways, bobs, breathes and pulls back
//! from walls (see [`holder`]).

pub mod actions;
pub mod base_clip;
pub mod clip_hold;
pub mod contact;
pub mod gun;
pub mod gun_clips;
pub mod handling;
pub mod holder;
pub mod leftovers;
#[cfg(test)]
mod legacy_plan;
pub mod mechanism;
#[cfg(test)]
mod pack_tests;
pub mod playing;
pub mod preview;
pub mod templates;
mod values;

pub(crate) use values::register;

pub use gun::{Gun, GunKind, HandRole, Socket};
pub use holder::{GunHolder, GunHolderState, Shot, fire};
