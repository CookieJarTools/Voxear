//! The Guns module: guns held in a character's own hands, every motion of
//! them made as it happens rather than played from clips. A Gun is data
//! on a gun's model: its kind, weight and sockets. A Gun Holder holds one
//! in front of its First-Person View, and aims, sways, bobs, breathes and
//! pulls back from walls (see [`holder`]).

pub mod actions;
pub mod contact;
pub mod gun;
pub mod hands;
pub mod holder;
pub mod leftovers;
pub mod mechanism;
pub mod playing;
mod values;

pub(crate) use values::register;

pub use gun::{Gun, GunKind, HandRole, Socket};
pub use holder::{GunHolder, GunHolderState, Shot, fire};
