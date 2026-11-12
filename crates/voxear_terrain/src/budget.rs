//! What making worlds of blocks may spend, so a game runs on slow machines
//! as well as fast ones: how many threads make terrain at once, how many
//! chunk columns are made at once, how many megabytes of chunks go to the
//! GPU a frame, and how far a world without end is drawn, near and coarse.
//! A preset ([`WorldQuality`]) sets all of them; changing one makes it
//! Custom. Kept in the project's `voxear.toml` and changed by a game's
//! options as it runs:
//!
//! ```toml
//! [world]
//! quality = "low"
//! ```

use serde::{Deserialize, Serialize};
use voxear_ecs::World;

/// A preset of [`WorldBudget`]s, from Low, for slow machines, to Ultra; and
/// Custom, once any number is changed by hand.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorldQuality {
    Low,
    Medium,
    #[default]
    High,
    Ultra,
    Custom,
}

impl WorldQuality {
    pub const ALL: [WorldQuality; 5] =
        [WorldQuality::Low, WorldQuality::Medium, WorldQuality::High, WorldQuality::Ultra, WorldQuality::Custom];

    pub fn label(self) -> &'static str {
        match self {
            WorldQuality::Low => "Low",
            WorldQuality::Medium => "Medium",
            WorldQuality::High => "High",
            WorldQuality::Ultra => "Ultra",
            WorldQuality::Custom => "Custom",
        }
    }

    /// The preset's numbers; none for Custom, which keeps whatever they are.
    pub fn budget(self) -> Option<WorldBudget> {
        let (threads, columns, upload, view_distance, far_distance) = match self {
            WorldQuality::Low => (2, 2, 4, 64.0, 512.0),
            WorldQuality::Medium => (4, 8, 8, 128.0, 1024.0),
            WorldQuality::High => (8, 24, 16, 192.0, 2048.0),
            WorldQuality::Ultra => (16, 48, 32, 256.0, 4096.0),
            WorldQuality::Custom => return None,
        };
        Some(WorldBudget { quality: self, threads, columns, upload, view_distance, far_distance })
    }
}

/// What making worlds of blocks may spend (see the module's notes). A world
/// without end draws as far as its own distances say, and no further than
/// these.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WorldBudget {
    pub quality: WorldQuality,
    /// How many threads make terrain at once.
    pub threads: u32,
    /// How many chunk columns a world without end makes at once.
    pub columns: u32,
    /// How many megabytes of chunks go to the GPU a frame; a world of more
    /// comes over several frames, shown as it was until then.
    pub upload: u32,
    /// How far a world without end's own chunks reach, in metres.
    pub view_distance: f32,
    /// How far its coarser chunks reach, in metres.
    pub far_distance: f32,
}

impl Default for WorldBudget {
    fn default() -> Self {
        WorldQuality::High.budget().expect("a preset")
    }
}

impl WorldBudget {
    /// Every number within what it may be, a preset's numbers its own:
    /// threads 1 to 64, columns 1 to 256, megabytes 1 to 512, a view
    /// distance of 32 m to 4 km and a far distance no shorter than it, up
    /// to 64 km.
    pub fn checked(self) -> Self {
        if let Some(preset) = self.quality.budget() {
            return preset;
        }
        let view_distance = finite_or(self.view_distance, 128.0).clamp(32.0, 4096.0);
        Self {
            quality: WorldQuality::Custom,
            threads: self.threads.clamp(1, 64),
            columns: self.columns.clamp(1, 256),
            upload: self.upload.clamp(1, 512),
            view_distance,
            far_distance: finite_or(self.far_distance, 1024.0).clamp(view_distance, 65_536.0),
        }
    }

    /// The budget in `world`, or the default where it has none.
    pub fn of(world: &World) -> Self {
        world.resource::<WorldBudget>().copied().unwrap_or_default()
    }

    /// Puts the budget where the streamer and the renderer read it, and
    /// lets the terrain's threads work as many at once as it says.
    pub fn put(self, world: &mut World) {
        let budget = self.checked();
        crate::workers::set_threads(budget.threads as usize);
        world.insert_resource(budget);
    }

    /// How many bytes of chunks go to the GPU a frame.
    pub fn upload_bytes(&self) -> u64 {
        u64::from(self.upload.max(1)) << 20
    }
}

fn finite_or(value: f32, otherwise: f32) -> f32 {
    if value.is_finite() { value } else { otherwise }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preset_sets_every_number_and_a_number_changed_by_hand_is_custom_and_kept_within_bounds() {
        let low = WorldQuality::Low.budget().unwrap();
        assert_eq!((low.threads, low.columns), (2, 2), "two columns at a time on two threads");
        // A preset's numbers are its own, whatever was written beside it.
        let written: WorldBudget = toml::from_str("quality = \"low\"\nthreads = 12").unwrap();
        assert_eq!(written.checked(), low);
        // Custom keeps its own, within bounds.
        let custom = WorldBudget { quality: WorldQuality::Custom, threads: 0, far_distance: 10.0, ..low }.checked();
        assert_eq!((custom.threads, custom.view_distance, custom.far_distance), (1, 64.0, 64.0));
        assert_eq!(WorldBudget::default().quality, WorldQuality::High);
        // Read back as written.
        let text = toml::to_string(&custom).unwrap();
        assert_eq!(toml::from_str::<WorldBudget>(&text).unwrap(), custom);
    }
}
