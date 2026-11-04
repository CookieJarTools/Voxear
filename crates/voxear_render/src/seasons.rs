//! Seasons for plants: a Seasons component on an Obj turns the level's
//! plants through the year, as The Vegetation Engine's seasons do in Unity.
//! The shader is `shaders/seasons.slang`.
//!
//! Leaves (materials marked foliage) take each season's colour, keeping their
//! texture's light and dark; every plant (foliage, or a material that moves
//! in the wind, such as bark) takes snow on what faces up and darkens and
//! shines when wet. Plants across a field turn a little earlier or later
//! than each other, and patches of them dry out, by noise over the level, so
//! a field is never one flat colour.

use serde::{Deserialize, Serialize};
use voxear_ecs::Component;

use crate::gpu::GpuSeasons;

/// The four seasons, in the order the year turns through them. A Seasons'
/// `time_of_year` is one of these as a number, or between two of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

impl Season {
    pub const ALL: [Season; 4] = [Season::Spring, Season::Summer, Season::Autumn, Season::Winter];

    /// Where the season falls in the year, as `Seasons::time_of_year` reads.
    pub fn time(self) -> f32 {
        self as usize as f32
    }

    pub fn name(self) -> &'static str {
        match self {
            Season::Spring => "Spring",
            Season::Summer => "Summer",
            Season::Autumn => "Autumn",
            Season::Winter => "Winter",
        }
    }
}

/// What a foliage material's leaves do through the year.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Leaves {
    /// They turn each season's colour, as most plants' do.
    #[default]
    Turn,
    /// They stay as made all year, as a pine's needles do.
    Stay,
}

impl Leaves {
    pub const ALL: [Leaves; 2] = [Leaves::Turn, Leaves::Stay];

    pub fn name(self) -> &'static str {
        match self {
            Leaves::Turn => "Turn",
            Leaves::Stay => "Stay",
        }
    }

    /// The material flags shaders read it by.
    pub(crate) fn flags(self) -> u32 {
        match self {
            Leaves::Turn => 0,
            Leaves::Stay => crate::gpu::MATERIAL_LEAVES_STAY,
        }
    }
}

/// How plants look at the height of one season.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SeasonLook {
    /// The colour leaves turn, in sRGB as people pick colours. The leaves'
    /// texture keeps its light and dark under it.
    pub color: [f32; 3],
    /// How far leaves turn to `color`, from 0 for their own colour to 1.
    pub color_amount: f32,
    /// How much snow lies on plants, on what faces up, from 0 to 1.
    pub snow: f32,
}

impl Default for SeasonLook {
    fn default() -> Self {
        Self { color: [0.3, 0.5, 0.15], color_amount: 0.0, snow: 0.0 }
    }
}

/// The level's seasons, for its plants. The first one in the level is used.
/// Its own defaults leave plants as they are made in summer, and turn them
/// as `time_of_year` moves: set it to a season with [`Seasons::set`], or
/// move it a little each frame to turn the year.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Seasons {
    /// Where the year is, from 0 to 4: spring at 0, summer at 1, autumn at
    /// 2, winter at 3 and spring again at 4. Between two, plants are part
    /// way from one to the next.
    pub time_of_year: f32,
    pub spring: SeasonLook,
    pub summer: SeasonLook,
    pub autumn: SeasonLook,
    pub winter: SeasonLook,
    /// How unevenly plants follow the year, from 0 for all together to 1
    /// for up to half a season early or late, and how much their shades
    /// differ.
    pub variation: f32,
    /// How wide the patches that turn together, and dry together, are
    /// across, in metres.
    pub patch_size: f32,
    /// How much of a field dries out in patches, from 0 to 1.
    pub dryness: f32,
    /// The colour dry leaves turn, in sRGB.
    pub dry_color: [f32; 3],
    /// How wet plants are, from 0 to 1: darker, and shining.
    pub wetness: f32,
}

impl Component for Seasons {}

impl Default for Seasons {
    fn default() -> Self {
        Self {
            time_of_year: Season::Summer.time(),
            spring: SeasonLook { color: [0.45, 0.68, 0.2], color_amount: 0.35, snow: 0.0 },
            summer: SeasonLook { color: [0.3, 0.5, 0.15], color_amount: 0.0, snow: 0.0 },
            autumn: SeasonLook { color: [0.8, 0.45, 0.14], color_amount: 0.8, snow: 0.0 },
            winter: SeasonLook { color: [0.42, 0.3, 0.18], color_amount: 0.9, snow: 0.6 },
            variation: 0.5,
            patch_size: 25.0,
            dryness: 0.0,
            dry_color: [0.62, 0.52, 0.25],
            wetness: 0.0,
        }
    }
}

impl Seasons {
    /// Turns the year to the height of `season`.
    pub fn set(&mut self, season: Season) {
        self.time_of_year = season.time();
    }

    /// The season the year is nearest the height of.
    pub fn season(&self) -> Season {
        Season::ALL[(self.time_of_year.rem_euclid(4.0).round() as usize) % 4]
    }

    pub fn look(&self, season: Season) -> &SeasonLook {
        match season {
            Season::Spring => &self.spring,
            Season::Summer => &self.summer,
            Season::Autumn => &self.autumn,
            Season::Winter => &self.winter,
        }
    }

    pub fn look_mut(&mut self, season: Season) -> &mut SeasonLook {
        match season {
            Season::Spring => &mut self.spring,
            Season::Summer => &mut self.summer,
            Season::Autumn => &mut self.autumn,
            Season::Winter => &mut self.winter,
        }
    }

    /// The seasons as shaders read them.
    pub(crate) fn gpu(&self) -> GpuSeasons {
        let mut gpu = GpuSeasons { enabled: 1, ..GpuSeasons::default() };
        for (i, season) in Season::ALL.into_iter().enumerate() {
            let look = self.look(season);
            let amount = look.color_amount.clamp(0.0, 1.0);
            // Weighted by how far leaves turn, so blending two seasons blends what they show.
            let [r, g, b] = linear(look.color).map(|c| c * amount);
            gpu.colors[i] = [r, g, b, amount];
            gpu.looks[i] = [0.0, look.snow.clamp(0.0, 1.0), 0.0, 0.0];
        }
        let [r, g, b] = linear(self.dry_color);
        gpu.dry = [r, g, b, self.dryness.clamp(0.0, 1.0)];
        gpu.time = self.time_of_year.rem_euclid(4.0);
        gpu.variation = self.variation.clamp(0.0, 1.0);
        gpu.patch_size = self.patch_size.max(0.1);
        gpu.wetness = self.wetness.clamp(0.0, 1.0);
        gpu
    }
}

fn linear(srgb: [f32; 3]) -> [f32; 3] {
    srgb.map(|c| {
        let c = c.clamp(0.0, 1.0);
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_year_wraps_and_seasons_read_back() {
        let mut seasons = Seasons::default();
        assert_eq!(seasons.season(), Season::Summer);
        seasons.set(Season::Winter);
        assert_eq!(seasons.season(), Season::Winter);
        seasons.time_of_year = 3.8;
        assert_eq!(seasons.season(), Season::Spring, "spring comes round again");
        seasons.time_of_year = -1.0;
        assert_eq!(seasons.gpu().time, 3.0);
    }

    #[test]
    fn winter_is_snowy_and_summer_as_made() {
        let gpu = Seasons::default().gpu();
        assert_eq!(gpu.enabled, 1);
        assert_eq!(gpu.colors[1][3], 0.0, "summer leaves keep their own colour");
        assert!(gpu.looks[3][1] > 0.0);
    }
}
