//! The two components a cave room's prefab is made of: Cave Shape, on each
//! Obj that hollows part of the room or fills part back in, and Cave Exit,
//! on each Obj where a tunnel may join it.
//!
//! They do nothing by themselves in a level. The editor reads a prefab's
//! into the numbers a Cave Network keeps for the room (see `rooms`), and
//! draws them in the Scene view so a room is built with the usual tools:
//! an Obj's position, rotation and scale place, turn and size its shape.

use serde::{Deserialize, Serialize};
use voxear_ecs::Component;

use super::rooms::RoomForm;

/// Whether a shape takes rock out or puts it back.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaveMode {
    #[default]
    Carve,
    Fill,
}

/// One shape of a cave room: as wide, high and long as `size` times its
/// Obj's scale, in metres.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CaveShape {
    pub shape: RoomForm,
    pub mode: CaveMode,
    pub size: [f32; 3],
    /// How likely a room placed in the ground has it, 0 to 1.
    pub chance: f32,
    /// How far in from its surface the walls' roughness may come, in
    /// metres.
    pub margin: f32,
}

impl Component for CaveShape {}

impl Default for CaveShape {
    fn default() -> Self {
        Self { shape: RoomForm::HalfBall, mode: CaveMode::Carve, size: [10.0, 5.0, 10.0], chance: 1.0, margin: 0.5 }
    }
}

/// Where a tunnel may join a cave room: at its Obj, leaving along the
/// Obj's forward.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CaveExit {}

impl Component for CaveExit {}
