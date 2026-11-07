//! Guns held in the editor, to see them as a designer works on them: a
//! holder with a Hold Preview is held, and its view placed, even while
//! editing, where no other holder in the edited level is. Its actions play
//! from an ammo state the designer picks, or are shown at a time within one,
//! the same every time that time is shown. A preview throws nothing away:
//! cases and magazines would be left lying in the edited level.

use voxear_ecs::{Component, Entity, World};

use super::actions::ActionId;
use super::gun::GunKind;
use super::mechanism::{Chamber, GunState};

/// Marks a holder the editor previews.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HoldPreview {
    /// The ammo state its actions start from.
    pub start: PreviewStart,
    /// An action shown at a time within it, in seconds, every frame; none
    /// plays as the holder is asked.
    pub at: Option<(ActionId, f32)>,
}

impl Component for HoldPreview {}

/// The ammo state a preview's actions start from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PreviewStart {
    /// As the gun is now.
    #[default]
    AsItIs,
    /// Full, a round chambered.
    Full,
    /// Half the feed left, a round chambered.
    Half,
    /// The feed empty, a round still chambered.
    Empty,
    /// Nothing in the feed or the chamber, the action open where it locks
    /// back empty.
    Dry,
}

impl PreviewStart {
    pub const ALL: [PreviewStart; 5] =
        [PreviewStart::AsItIs, PreviewStart::Full, PreviewStart::Half, PreviewStart::Empty, PreviewStart::Dry];

    /// Its name, as the editor shows it.
    pub fn title(self) -> &'static str {
        match self {
            PreviewStart::AsItIs => "As It Is",
            PreviewStart::Full => "Full",
            PreviewStart::Half => "Half",
            PreviewStart::Empty => "Empty",
            PreviewStart::Dry => "Dry",
        }
    }

    /// A gun of `kind` in this state, its parts at rest; `now` as it is.
    pub fn state(self, kind: GunKind, now: &GunState) -> GunState {
        let parts = now.parts.iter().map(|&(entity, _, rest)| (entity, 0.0, rest)).collect();
        let loaded = GunState { parts, ..GunState::loaded(kind) };
        let live = if kind.open_bolt() { Chamber::Empty } else { Chamber::Live };
        match self {
            PreviewStart::AsItIs => now.clone(),
            PreviewStart::Full => loaded,
            PreviewStart::Half => GunState { rounds: kind.capacity() / 2, ..loaded },
            PreviewStart::Empty => GunState { rounds: 0, chamber: live, ..loaded },
            PreviewStart::Dry => {
                GunState { rounds: 0, chamber: Chamber::Empty, open: kind.self_loading() || kind.open_bolt(), ..loaded }
            }
        }
    }
}

/// Whether a holder, or a view, is posed now: always while playing, and
/// while editing only one the editor previews.
pub fn runs(world: &World, entity: Entity) -> bool {
    !crate::editing(world) || world.has::<HoldPreview>(entity)
}
