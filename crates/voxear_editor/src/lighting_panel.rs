//! The Lighting page, in the rail's drawer, as Unity's Lighting window: the
//! lighting settings the active scene names, picked from none, the
//! built-in ones and the project's `.lighting` files, and below, a tab for
//! each group of their settings, each changed in place.
//!
//! Each setting shows what it is in effect, the look's value where the file
//! writes none, and a change writes only that setting into the file,
//! keeping its comments, so the open scene re-lights at once. Emptying a
//! number's field takes the setting out of the file, back to the look's.
//! Built-in lighting cannot be changed: a copy of it can.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use voxear_assets::{AssetKind, AssetRef};
use voxear_engine::{
    Game,
    lighting::LightingFile,
    scene_file::{self, PostTable, SkyData, SkyFromLook, SkyModeData, sky_data},
};
use voxear_render::{LookName, PostProcess, Sky};
use voxear_ui::{Color, NodeId, Style, Ui};

/// What a setting is edited with.
#[derive(Clone, Copy)]
enum Kind {
    Toggle,
    /// One of these, each as the menu shows it and as the file writes it;
    /// an empty value takes the setting out of the file.
    Choice(&'static [(&'static str, &'static str)]),
    /// None, or one of the built-in looks (`LookName::ALL`).
    Look,
    Number {
        min: f32,
        max: f32,
        unit: &'static str,
    },
    /// An sRGB colour, picked from its swatch.
    Color,
    /// A whole number from 0 to `max`.
    Whole {
        max: u32,
    },
    /// A line of text.
    Text,
}

/// One setting: where it is in a `.lighting` file, its name, its group, and
/// when it shows.
struct Info {
    path: &'static [&'static str],
    title: &'static str,
    group: &'static str,
    kind: Kind,
    /// What must hold for the setting to do anything, each a setting and
    /// its value as the file writes it (`true` for a toggle that is on).
    /// While any does not, the setting is hidden, so the page shows only
    /// what changes the picture.
    when: When,
    /// What it does, in plain words, shown when the pointer rests on its
    /// name.
    help: &'static str,
}

type When = &'static [(&'static [&'static str], &'static str)];

const ALWAYS: When = &[];
const GRADIENT_SKY: When = &[(&["sky", "mode"], "gradient")];
const ATMOSPHERE: When = &[(&["sky", "mode"], "atmosphere")];
const AUTO_EXPOSURE: When = &[(&["post", "exposure", "auto"], "true")];
const AGX: When = &[(&["post", "tonemap", "mapper"], "agx")];
const BLOOM: When = &[(&["post", "bloom", "enabled"], "true")];
const FOG: When = &[(&["post", "fog", "enabled"], "true")];
const SHAFTS: When = &[(&["post", "fog", "enabled"], "true"), (&["post", "fog", "volumetric"], "true")];
const BOUNCE: When = &[(&["post", "global_illumination", "enabled"], "true")];
const OCCLUSION: When = &[(&["post", "ambient_occlusion", "enabled"], "true")];
const FOCUS: When = &[(&["post", "depth_of_field", "enabled"], "true")];
const BLUR: When = &[(&["post", "motion_blur", "enabled"], "true")];

const fn number(min: f32, max: f32, unit: &'static str) -> Kind {
    Kind::Number { min, max, unit }
}

const WEATHER_KINDS: &[(&str, &str)] = &[
    ("Clear", "clear"),
    ("Cloudy", "cloudy"),
    ("Overcast", "overcast"),
    ("Rain", "rain"),
    ("Storm", "storm"),
    ("Snow", "snow"),
    ("Fog", "fog"),
    ("Blizzard", "blizzard"),
    ("Sandstorm", "sandstorm"),
    ("Custom", "custom"),
];
const CUSTOM_WEATHER: When = &[(&["weather", "kind"], "custom")];
const CLOUD_KINDS: &[(&str, &str)] = &[("Soft", "soft"), ("Blocks", "blocks")];
const BLOCK_CLOUDS: When = &[(&["sky", "clouds", "kind"], "blocks")];
const SKY_MODES: &[(&str, &str)] = &[("Gradient", "gradient"), ("Atmosphere", "atmosphere")];
const TONEMAPPERS: &[(&str, &str)] =
    &[("AgX", "agx"), ("ACES", "aces"), ("Neutral", "neutral"), ("Reinhard", "reinhard"), ("None", "none")];

#[rustfmt::skip]
const SETTINGS: &[Info] = &[
    Info { path: &["look"], title: "Look", group: "Sky", kind: Kind::Look, when: ALWAYS,
        help: "A finished lighting style to start from. Every other setting begins at the look's value until you change it." },
    Info { path: &["sky", "mode"], title: "Mode", group: "Sky", kind: Kind::Choice(SKY_MODES), when: ALWAYS,
        help: "Gradient paints the sky with three colours you pick. Atmosphere works the sky out from the sun, as real air does: blue by day, orange at sunset." },
    Info { path: &["sky", "zenith"], title: "Zenith", group: "Sky", kind: Kind::Color, when: GRADIENT_SKY,
        help: "The sky's colour straight overhead." },
    Info { path: &["sky", "horizon"], title: "Horizon", group: "Sky", kind: Kind::Color, when: GRADIENT_SKY,
        help: "The sky's colour at the horizon, where it meets the ground." },
    Info { path: &["sky", "ground"], title: "Ground", group: "Sky", kind: Kind::Color, when: GRADIENT_SKY,
        help: "The colour below the horizon, which also lights things from beneath." },
    Info { path: &["sky", "ambient"], title: "Ambient", group: "Sky", kind: number(0.0, 8.0, ""), when: ALWAYS,
        help: "How brightly the sky lights everything, shade included. Lower it for a darker, moodier scene." },
    Info { path: &["sky", "haze"], title: "Haze", group: "Sky", kind: number(0.0, 10.0, ""), when: ATMOSPHERE,
        help: "How dusty or misty the air is. More haze makes the distance fade and the sky glow round the sun." },
    Info { path: &["sky", "haze_tint"], title: "Haze tint", group: "Sky", kind: Kind::Color, when: ATMOSPHERE,
        help: "The colour of the haze: warm for dust, white for mist." },
    Info { path: &["sky", "haze_glow"], title: "Haze glow", group: "Sky", kind: number(0.0, 0.95, ""), when: ATMOSPHERE,
        help: "How tightly the haze glows round the sun. Higher makes a small bright halo; lower spreads it over the sky." },
    Info { path: &["sky", "distance_haze"], title: "Distance haze", group: "Sky", kind: number(0.0, 100.0, ""), when: ATMOSPHERE,
        help: "How quickly far-away things fade into the haze. Higher hides the distance sooner." },
    Info { path: &["sky", "sun_size"], title: "Sun size", group: "Sky", kind: number(0.05, 20.0, "°"), when: ALWAYS,
        help: "How big the sun's disc looks in the sky, in degrees." },
    Info { path: &["sky", "stars"], title: "Stars", group: "Sky", kind: number(0.0, 4.0, ""), when: ALWAYS,
        help: "How bright the stars are at night. 0 hides them." },
    Info { path: &["sky", "clouds", "kind"], title: "Kind", group: "Clouds", kind: Kind::Choice(CLOUD_KINDS), when: ALWAYS,
        help: "Soft clouds have billowing edges. Blocks builds the same clouds out of cubes, flat-sided, for a world of blocks." },
    Info { path: &["sky", "clouds", "block"], title: "Block size", group: "Clouds", kind: number(1.0, 2000.0, "m"), when: BLOCK_CLOUDS,
        help: "How wide one cube of the clouds is." },
    Info { path: &["sky", "clouds", "softness"], title: "Softness", group: "Clouds", kind: number(0.0, 1.0, ""), when: BLOCK_CLOUDS,
        help: "How soft the cubes are: 0 for solid cubes, 1 for cubes of mist, faint at a cloud's edges where the sky shows through." },
    Info { path: &["sky", "clouds", "cover"], title: "Cover", group: "Clouds", kind: number(0.0, 1.0, ""), when: ALWAYS,
        help: "How much of the sky the clouds fill: 0 for none, 0.4 for a fair day, 1 for overcast." },
    Info { path: &["sky", "clouds", "height"], title: "Height", group: "Clouds", kind: number(0.0, 10000.0, "m"), when: ALWAYS,
        help: "How high the clouds' undersides are above the level's ground." },
    Info { path: &["sky", "clouds", "thickness"], title: "Thickness", group: "Clouds", kind: number(10.0, 5000.0, "m"), when: ALWAYS,
        help: "How tall the clouds are, from their undersides to their tops." },
    Info { path: &["sky", "clouds", "size"], title: "Size", group: "Clouds", kind: number(10.0, 10000.0, "m"), when: ALWAYS,
        help: "How far across one cloud is." },
    Info { path: &["sky", "clouds", "density"], title: "Density", group: "Clouds", kind: number(0.0, 4.0, ""), when: ALWAYS,
        help: "How thick the clouds are. Lower lets more light through them; higher makes them heavy and dark underneath." },
    Info { path: &["sky", "clouds", "drift"], title: "Drift", group: "Clouds", kind: number(0.0, 200.0, "m/s"), when: ALWAYS,
        help: "How fast the clouds move. They go the way the level's Wind Zone blows." },
    Info { path: &["sky", "clouds", "shadows"], title: "Shadows", group: "Clouds", kind: number(0.0, 1.0, ""), when: ALWAYS,
        help: "How dark the clouds' shadows on the world are. 0 turns them off." },
    Info { path: &["weather", "kind"], title: "Weather", group: "Weather", kind: Kind::Choice(WEATHER_KINDS), when: ALWAYS,
        help: "The weather the level is in when lit by these settings. Clear leaves the lighting as it is; the others add cloud, gloom, fog, wind, rain or snow over it." },
    Info { path: &["weather", "cloud"], title: "Cloud", group: "Weather", kind: number(0.0, 1.0, ""), when: CUSTOM_WEATHER,
        help: "How far the sky's clouds are filled in: 1 is overcast whatever the Clouds tab's Cover." },
    Info { path: &["weather", "darkness"], title: "Darkness", group: "Weather", kind: number(0.0, 1.0, ""), when: CUSTOM_WEATHER,
        help: "How heavy and dark the clouds are, and how much of the sun's light they take." },
    Info { path: &["weather", "fog"], title: "Fog", group: "Weather", kind: number(0.0, 1.0, ""), when: CUSTOM_WEATHER,
        help: "How much fog and haze the weather adds to the air." },
    Info { path: &["weather", "wind"], title: "Wind", group: "Weather", kind: number(0.0, 1.0, ""), when: CUSTOM_WEATHER,
        help: "How much harder the level's Wind Zone blows: plants, the sea, the clouds and the rain all feel it." },
    Info { path: &["weather", "rain"], title: "Rain", group: "Weather", kind: number(0.0, 1.0, ""), when: CUSTOM_WEATHER,
        help: "How hard it rains round the camera. Rain soaks every surface, which dries once it stops." },
    Info { path: &["weather", "snow"], title: "Snow", group: "Weather", kind: number(0.0, 1.0, ""), when: CUSTOM_WEATHER,
        help: "How hard it snows round the camera. Snow settles on what faces up, and melts once it stops." },
    Info { path: &["weather", "lightning"], title: "Lightning", group: "Weather", kind: number(0.0, 1.0, ""), when: CUSTOM_WEATHER,
        help: "How often lightning strikes, lighting the sky and the level for a moment: at 1, every eight seconds or so." },
    Info { path: &["weather", "dust"], title: "Dust", group: "Weather", kind: number(0.0, 1.0, ""), when: CUSTOM_WEATHER,
        help: "How much blown sand is in the air: thick, dun and dim." },
    Info { path: &["sun", "temperature"], title: "Temperature", group: "Sun", kind: number(1000.0, 20000.0, "K"), when: ALWAYS,
        help: "The sun's colour as a temperature, in kelvin: about 2000 K is a candle-orange sunset, 5500 K is white noon light, above 7000 K is cool and blue." },
    Info { path: &["sun", "tint"], title: "Tint", group: "Sun", kind: Kind::Color, when: ALWAYS,
        help: "A colour the sunlight is filtered through, on top of its temperature. White leaves it as it is." },
    Info { path: &["sun", "strength"], title: "Strength", group: "Sun", kind: number(0.0, 10.0, "×"), when: ALWAYS,
        help: "How bright the sun is, times its light's own intensity. 1 leaves it as set." },
    Info { path: &["sun", "softness"], title: "Softness", group: "Sun", kind: number(0.0, 20.0, "°"), when: ALWAYS,
        help: "How soft the sun's shadows are, as its width in degrees. 0 gives hard, crisp shadows; higher blurs their edges, as through thin cloud." },
    Info { path: &["sun", "shadow_tint"], title: "Shadow tint", group: "Sun", kind: Kind::Color, when: ALWAYS,
        help: "The colour shadows lean towards. Black leaves them neutral; a warm brown or cool blue gives shade a mood." },
    Info { path: &["sun", "shadow_distance"], title: "Shadow distance", group: "Sun", kind: number(5.0, 1000.0, "m"), when: ALWAYS,
        help: "How far from the camera the sun casts shadows. Nearer keeps the shadows by the camera sharper; farther shows them over a wider view, each softer." },
    Info { path: &["sun", "shadow_split"], title: "Cascade split", group: "Sun", kind: number(0.0, 1.0, ""), when: ALWAYS,
        help: "How the sun's four shadow maps share the shadow distance. Near 1, the nearest is small and very sharp and the farthest wide; near 0, they share it evenly, so far shadows are sharper and near ones softer." },
    Info { path: &["post", "exposure", "auto"], title: "Auto exposure", group: "Exposure", kind: Kind::Toggle, when: ALWAYS,
        help: "Lets the camera adjust its brightness to the scene, like an eye: brighter in dark places, darker in bright ones. Off keeps it fixed." },
    Info { path: &["post", "exposure", "compensation"], title: "Compensation", group: "Exposure", kind: number(-8.0, 8.0, "EV"), when: ALWAYS,
        help: "Makes the whole picture brighter (positive) or darker (negative), in stops. Each stop doubles or halves the brightness." },
    Info { path: &["post", "exposure", "min_ev"], title: "Min exposure", group: "Exposure", kind: number(-16.0, 16.0, "EV"), when: AUTO_EXPOSURE,
        help: "The most auto exposure may darken the picture, in stops, for very bright scenes." },
    Info { path: &["post", "exposure", "max_ev"], title: "Max exposure", group: "Exposure", kind: number(-16.0, 16.0, "EV"), when: AUTO_EXPOSURE,
        help: "The most auto exposure may brighten the picture, in stops, for dark scenes. Raise it for rooms lit only by their windows." },
    Info { path: &["post", "exposure", "speed_up"], title: "Adapt to light", group: "Exposure", kind: number(0.0, 20.0, "EV/s"), when: AUTO_EXPOSURE,
        help: "How fast the eye adjusts when you walk into somewhere brighter, in stops a second." },
    Info { path: &["post", "exposure", "speed_down"], title: "Adapt to dark", group: "Exposure", kind: number(0.0, 20.0, "EV/s"), when: AUTO_EXPOSURE,
        help: "How fast the eye adjusts when you walk into somewhere darker, in stops a second. Eyes adjust to the dark slowly." },
    Info { path: &["post", "tonemap", "mapper"], title: "Tonemapper", group: "Colour", kind: Kind::Choice(TONEMAPPERS), when: ALWAYS,
        help: "How bright light is squeezed into what a screen can show. AgX keeps colours natural in bright light and is the best default; ACES is punchier, like film." },
    Info { path: &["post", "tonemap", "contrast"], title: "Contrast", group: "Colour", kind: number(0.0, 3.0, ""), when: AGX,
        help: "The difference between light and dark. Higher is punchier, lower is flatter. 1 is AgX's own." },
    Info { path: &["post", "tonemap", "saturation"], title: "Saturation", group: "Colour", kind: number(0.0, 3.0, ""), when: AGX,
        help: "How strong colours are. Higher is more vivid, lower towards grey. 1 is AgX's own." },
    Info { path: &["post", "grading", "temperature"], title: "White balance", group: "Colour", kind: number(-1.0, 1.0, ""), when: ALWAYS,
        help: "Shifts the whole picture warmer (orange) or cooler (blue), as a camera's white balance does." },
    Info { path: &["post", "grading", "tint"], title: "Green-magenta", group: "Colour", kind: number(-1.0, 1.0, ""), when: ALWAYS,
        help: "Shifts the whole picture towards green (negative) or magenta (positive)." },
    Info { path: &["post", "grading", "contrast"], title: "Grade contrast", group: "Colour", kind: number(0.0, 3.0, ""), when: ALWAYS,
        help: "Contrast applied in the final colour grade, after the tonemapper." },
    Info { path: &["post", "grading", "saturation"], title: "Grade saturation", group: "Colour", kind: number(0.0, 3.0, ""), when: ALWAYS,
        help: "Colour strength applied in the final colour grade, after the tonemapper." },
    Info { path: &["post", "vignette", "intensity"], title: "Vignette", group: "Colour", kind: number(0.0, 1.0, ""), when: ALWAYS,
        help: "Darkens the corners of the picture, drawing the eye to the middle, as old lenses do." },
    Info { path: &["post", "chromatic_aberration"], title: "Colour fringes", group: "Colour", kind: number(0.0, 1.0, ""), when: ALWAYS,
        help: "Splits colours slightly at the picture's edges, as a cheap lens does. Small amounts look filmic." },
    Info { path: &["post", "bloom", "enabled"], title: "Bloom", group: "Bloom", kind: Kind::Toggle, when: ALWAYS,
        help: "Makes bright things glow softly into what is around them: lamps, the sun, shiny highlights." },
    Info { path: &["post", "bloom", "intensity"], title: "Intensity", group: "Bloom", kind: number(0.0, 1.0, ""), when: BLOOM,
        help: "How much bright things glow." },
    Info { path: &["post", "bloom", "scatter"], title: "Scatter", group: "Bloom", kind: number(0.0, 1.0, ""), when: BLOOM,
        help: "How far the glow spreads. Low keeps it tight round the light; high spreads a wide haze." },
    Info { path: &["post", "bloom", "tint"], title: "Tint", group: "Bloom", kind: Kind::Color, when: BLOOM,
        help: "The colour of the glow. White keeps each light's own colour." },
    Info { path: &["post", "fog", "enabled"], title: "Fog", group: "Fog", kind: Kind::Toggle, when: ALWAYS,
        help: "Fills the air with fog that thickens with distance and towards the ground." },
    Info { path: &["post", "fog", "density"], title: "Density", group: "Fog", kind: number(0.0, 1.0, ""), when: FOG,
        help: "How thick the fog is. Small numbers go a long way: 0.01 is a light mist." },
    Info { path: &["post", "fog", "height_falloff"], title: "Height falloff", group: "Fog", kind: number(0.0, 2.0, ""), when: FOG,
        help: "How quickly the fog thins as it rises. Higher keeps it low to the ground." },
    Info { path: &["post", "fog", "base_height"], title: "Base height", group: "Fog", kind: number(-1000.0, 1000.0, "m"), when: FOG,
        help: "The height, in metres, where the fog is thickest." },
    Info { path: &["post", "fog", "sun_glow"], title: "Sun glow", group: "Fog", kind: number(0.0, 1.0, ""), when: FOG,
        help: "How brightly the fog glows when you look towards the sun." },
    Info { path: &["post", "fog", "volumetric"], title: "Light shafts", group: "Fog", kind: Kind::Toggle, when: FOG,
        help: "Lights the fog near the camera, so sunbeams and lamp glows show in the air, cut by shadows. It costs a little more." },
    Info { path: &["post", "fog", "volumetric_distance"], title: "Shaft distance", group: "Fog", kind: number(1.0, 512.0, "m"), when: SHAFTS,
        help: "How far from the camera light shafts reach, in metres. Beyond it the fog is plain." },
    Info { path: &["post", "fog", "anisotropy"], title: "Forward glow", group: "Fog", kind: number(-0.95, 0.95, ""), when: SHAFTS,
        help: "How much the fog glows looking towards a light. Higher gives brighter shafts towards the sun; 0 glows evenly every way." },
    Info { path: &["post", "global_illumination", "enabled"], title: "Bounced light", group: "Shading", kind: Kind::Toggle, when: ALWAYS,
        help: "Light bounces off surfaces onto others, as in real life: a sunlit floor lights the ceiling, a red wall tints what is beside it. It costs more, and looks much more natural." },
    Info { path: &["post", "global_illumination", "strength"], title: "Bounce strength", group: "Shading", kind: number(0.0, 4.0, ""), when: BOUNCE,
        help: "How bright the bounced light is. 1 is as physics has it." },
    Info { path: &["post", "global_illumination", "probe_spacing"], title: "Probe spacing", group: "Shading", kind: number(0.1, 16.0, "m"), when: BOUNCE,
        help: "How far apart, in metres, the points that measure bounced light are. Smaller catches more detail and costs more." },
    Info { path: &["post", "ambient_occlusion", "enabled"], title: "Ambient occlusion", group: "Shading", kind: Kind::Toggle, when: ALWAYS,
        help: "Darkens corners, cracks and where things meet, where less light reaches. It makes objects look grounded." },
    Info { path: &["post", "ambient_occlusion", "radius"], title: "Occlusion radius", group: "Shading", kind: number(0.0, 8.0, "m"), when: OCCLUSION,
        help: "How far, in metres, each point looks for nearby things that shade it. Larger darkens wider areas." },
    Info { path: &["post", "ambient_occlusion", "strength"], title: "Occlusion strength", group: "Shading", kind: number(0.0, 4.0, ""), when: OCCLUSION,
        help: "How dark the corners get." },
    Info { path: &["post", "contact_shadows", "enabled"], title: "Contact shadows", group: "Shading", kind: Kind::Toggle, when: ALWAYS,
        help: "Adds small, sharp shadows where things touch the ground or each other, that the sun's shadows are too coarse to show." },
    Info { path: &["post", "reflections", "enabled"], title: "Reflections", group: "Shading", kind: Kind::Toggle, when: ALWAYS,
        help: "Shiny surfaces reflect what is round them: polished floors, water, metal. With ray tracing they reflect everything, even what is off screen." },
    Info { path: &["post", "anti_aliasing", "specular_smoothing"], title: "Shimmer reduction", group: "Shading", kind: number(0.0, 1.0, ""), when: ALWAYS,
        help: "Calms the sparkle of shiny, finely bumpy surfaces, such as embroidery or worn metal, as the camera moves, by drawing them a little less glossy where their bumps are smaller than a pixel. 0 turns it off." },
    Info { path: &["post", "depth_of_field", "enabled"], title: "Depth of field", group: "Lens", kind: Kind::Toggle, when: ALWAYS,
        help: "Blurs what is nearer or farther than the focus distance, as a camera lens does." },
    Info { path: &["post", "depth_of_field", "focus_distance"], title: "Focus distance", group: "Lens", kind: number(0.0, 10000.0, "m"), when: FOCUS,
        help: "How far from the camera, in metres, things are in sharp focus." },
    Info { path: &["post", "depth_of_field", "aperture"], title: "Aperture", group: "Lens", kind: number(0.5, 64.0, "f"), when: FOCUS,
        help: "How much blur there is outside the focus: smaller f-numbers blur more, as on a real lens." },
    Info { path: &["post", "motion_blur", "enabled"], title: "Motion blur", group: "Lens", kind: Kind::Toggle, when: ALWAYS,
        help: "Blurs things as they move fast, or as the camera turns, as a film camera does." },
    Info { path: &["post", "motion_blur", "intensity"], title: "Blur intensity", group: "Lens", kind: number(0.0, 1.0, ""), when: BLUR,
        help: "How much moving things blur." },
];

/// What a look pack has that lighting settings have not: a line saying
/// what it looks like, shown first, and how Block Worlds' faces are drawn,
/// shown last.
#[rustfmt::skip]
const PACK_SETTINGS: &[Info] = &[
    Info { path: &["description"], title: "Description", group: "Pack", kind: Kind::Text, when: ALWAYS,
        help: "A line saying what the pack looks like, for the menu a player picks it from." },
    Info { path: &["water", "deep"], title: "Deep", group: "Water", kind: Kind::Color, when: ALWAYS,
        help: "The water's own colour, which it fades into where it is deep: the sea's, every Pool's and the water of Block Worlds." },
    Info { path: &["water", "scatter"], title: "Scatter", group: "Water", kind: Kind::Color, when: ALWAYS,
        help: "The colour light takes through the thin tops of waves and ripples, on the sea and Pools." },
    Info { path: &["water", "clarity"], title: "Clarity", group: "Water", kind: Kind::Color, when: ALWAYS,
        help: "What a metre of water lets through, by colour, in the sea and Pools. Nearer white is clearer water." },
    Info { path: &["water", "murk"], title: "Murk", group: "Water", kind: number(0.0, 20.0, ""), when: ALWAYS,
        help: "How murky the sea and Pools are: 1 as Clarity says, more to see less far into them." },
    Info { path: &["water", "foam"], title: "Foam", group: "Water", kind: number(0.0, 8.0, ""), when: ALWAYS,
        help: "How much foam the sea's breaking crests have. 0 for none." },
    Info { path: &["blocks", "voxels"], title: "Voxels", group: "Blocks", kind: Kind::Whole { max: 64 }, when: ALWAYS,
        help: "How many small cubes every Block World's faces are cut into along a block's side. 0 for plain faces, whatever a world's own setting. Empty leaves each world as it is." },
    Info { path: &["blocks", "relief"], title: "Relief", group: "Blocks", kind: number(0.0, 4.0, ""), when: ALWAYS,
        help: "How many small cubes deep the darkest parts of a block's picture sink." },
];

/// What a pack's menus say for a setting it leaves to the level.
const LEVELS_OWN: &str = "Level's own";

#[derive(Clone, Copy)]
enum Control {
    Toggle(NodeId),
    Choice(NodeId),
    Number(NodeId),
    Color(NodeId),
    /// A pack's toggle: the level's own, on or off.
    Tri(NodeId),
    Text(NodeId),
}

impl Control {
    fn node(self) -> NodeId {
        match self {
            Self::Toggle(node)
            | Self::Choice(node)
            | Self::Number(node)
            | Self::Color(node)
            | Self::Tri(node)
            | Self::Text(node) => node,
        }
    }
}

struct Row {
    info: &'static Info,
    row: NodeId,
    label: NodeId,
    control: Control,
    /// In a pack, a colour's button that gives the setting back to the level.
    reset: Option<NodeId>,
    /// Whether it is a look pack's row: its menus start with the level's
    /// own, and it shows nothing where the pack writes nothing.
    pack: bool,
}

/// What a change on the page asks of the editor.
#[derive(Clone, Debug)]
pub enum LightingAction {
    /// The active scene names other lighting settings, or none.
    SetScene(Option<String>),
    /// A setting of the lighting shown is written into its file, or taken
    /// out of it with `None`.
    Set { file: PathBuf, path: &'static [&'static str], value: Option<toml_edit::Value> },
    /// New lighting settings are made, a copy of those shown, and named in
    /// the active scene.
    Copy,
    /// The scene is shown under a look pack, as a player who picked it
    /// would see it, or under none. Nothing of it is saved.
    Wear(Option<String>),
    /// The scene is shown under a texture pack, as a player who picked it
    /// would see it, or under none. Nothing of it is saved.
    WearTextures(Option<String>),
}

/// Two changes are the same when they write the same text: the file's values
/// keep their own formatting, so they compare as written.
impl PartialEq for LightingAction {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::SetScene(a), Self::SetScene(b)) => a == b,
            (Self::Set { file, path, value }, Self::Set { file: f, path: p, value: v }) => {
                let text = |value: &Option<toml_edit::Value>| value.as_ref().map(|v| v.to_string());
                file == f && path == p && text(value) == text(v)
            }
            (Self::Copy, Self::Copy) => true,
            (Self::Wear(a), Self::Wear(b)) => a == b,
            (Self::WearTextures(a), Self::WearTextures(b)) => a == b,
            _ => false,
        }
    }
}

pub struct LightingPanel {
    /// The scene's lighting menu, what each of its entries names, a note
    /// under it, and the button that makes a copy.
    scene: NodeId,
    choices: Vec<Option<String>>,
    note: NodeId,
    copy: NodeId,
    /// The look pack the scene is shown under, and what each of its
    /// menu's entries names.
    look: NodeId,
    looks: Vec<Option<String>>,
    /// The texture pack the scene is shown under, and what each of its
    /// menu's entries names.
    textures: NodeId,
    packs: Vec<Option<String>>,
    fields: LightingFields,
    /// The lighting shown, as the scene names it.
    shown: Option<String>,
}

impl LightingPanel {
    /// Builds the scene's menu into `top`, a tab for each group into
    /// `tab_bar` and every setting into `view`.
    pub fn new(ui: &mut Ui, top: NodeId, tab_bar: NodeId, view: NodeId) -> Self {
        let row = ui.panel(top, Style::default());
        ui.add_class(row, "prop");
        let label = ui.add_label(row, "Lighting");
        ui.add_class(label, "prop-label");
        let scene = ui.dropdown(row, &["None"], 0);
        ui.add_class(scene, "value-choice");
        let note = ui.add_label(top, "");
        ui.add_class(note, "lighting-note");
        let copy = ui.button(top, "Make a Copy");
        ui.add_class(copy, "lighting-copy");
        let row = ui.panel(top, Style::default());
        ui.add_class(row, "prop");
        let label = ui.add_label(row, "Look pack");
        ui.add_class(label, "prop-label");
        let tip = "Shows the scene under a look pack, as a player who picked it would see it. It is not saved with \
                   the scene: players pick their own";
        ui.set_tooltip(label, tip, None);
        let look = ui.dropdown(row, &["None"], 0);
        ui.add_class(look, "value-choice");
        let row = ui.panel(top, Style::default());
        ui.add_class(row, "prop");
        let label = ui.add_label(row, "Texture pack");
        ui.add_class(label, "prop-label");
        let tip = "Shows the scene under a texture pack, as a player who picked it would see it. It is not saved \
                   with the scene: players pick their own";
        ui.set_tooltip(label, tip, None);
        let textures = ui.dropdown(row, &["None"], 0);
        ui.add_class(textures, "value-choice");
        let fields = LightingFields::new(ui, tab_bar, view);
        let packs = vec![None];
        Self { scene, choices: vec![None], note, copy, look, looks: vec![None], textures, packs, fields, shown: None }
    }

    /// Shows the active scene's lighting as it now is: the menu's choices,
    /// and every setting's value, which can be changed only in a file of
    /// the project's.
    pub fn refresh(&mut self, ui: &mut Ui, game: &mut Game) {
        let current = game.levels.scene_lighting().map(str::to_string);
        let own = world_own(game);
        self.show_scene(ui, game, current, own);
        self.show_looks(ui, game);
        self.show_textures(ui, game);
    }

    /// Lists the texture packs there are, the engine's and the project's,
    /// with the one the scene is shown under picked.
    fn show_textures(&mut self, ui: &mut Ui, game: &mut Game) {
        let worn = voxear_engine::texture_packs::worn(&game.content).map(str::to_string);
        let mut labels = vec!["None".to_string()];
        let mut packs = vec![None];
        for pack in voxear_engine::texture_packs::list(&mut game.content, None) {
            labels.push(pack.name);
            packs.push(Some(pack.reference));
        }
        let index = match packs.iter().position(|pack| *pack == worn) {
            Some(index) => index,
            // One worn that is not listed, as a player's own folder a script wore.
            None => {
                labels.push(worn.clone().unwrap_or_default());
                packs.push(worn);
                packs.len() - 1
            }
        };
        let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
        ui.set_options(self.textures, &labels);
        ui.set_selected_option(self.textures, index);
        self.packs = packs;
    }

    /// Lists the look packs there are, the engine's and the project's, with
    /// the one the scene is shown under picked.
    fn show_looks(&mut self, ui: &mut Ui, game: &mut Game) {
        let worn = voxear_engine::look_packs::worn(&game.engine.world).map(str::to_string);
        let packs = voxear_engine::look_packs::list(&mut game.content, None);
        let mut labels = vec!["None".to_string()];
        let mut looks = vec![None];
        for pack in packs {
            labels.push(pack.name);
            looks.push(Some(pack.reference));
        }
        let index = match looks.iter().position(|look| *look == worn) {
            Some(index) => index,
            // One worn that is not listed, as a player's own file a script wore.
            None => {
                labels.push(worn.clone().unwrap_or_default());
                looks.push(worn);
                looks.len() - 1
            }
        };
        let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
        ui.set_options(self.look, &labels);
        ui.set_selected_option(self.look, index);
        self.looks = looks;
    }

    /// Shows a scene's lighting: `current`, the lighting settings it names,
    /// or with none, `own`, its own sky and post-processing.
    pub fn show_scene(&mut self, ui: &mut Ui, game: &Game, current: Option<String>, own: Result<LightingFile>) {
        let mut labels = vec!["None".to_string()];
        let mut choices = vec![None];
        for (_, key, title) in LookName::ALL {
            labels.push(title.to_string());
            choices.push(Some(format!("builtin:{key}")));
        }
        for (label, pick) in crate::fields::assets_of(game, Some(AssetKind::Lighting)) {
            if let crate::fields::Pick::Asset(Some(path)) = pick {
                labels.push(label);
                choices.push(Some(path));
            }
        }
        let same = |choice: &Option<String>| match (choice, &current) {
            (Some(choice), Some(current)) => {
                choice == current || game.content.parse(choice).ok() == game.content.parse(current).ok()
            }
            (None, None) => true,
            _ => false,
        };
        let index = match choices.iter().position(same) {
            Some(index) => index,
            None => {
                labels.push(current.clone().unwrap_or_default());
                choices.push(current.clone());
                choices.len() - 1
            }
        };
        let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
        ui.set_options(self.scene, &labels);
        ui.set_selected_option(self.scene, index);
        self.choices = choices;

        self.shown = current.clone();
        let file = current.as_deref().and_then(|reference| project_file(game, reference));
        let read = match current.as_deref() {
            Some(reference) => read(game, reference),
            None => own,
        };
        let note = match (&current, &file, &read) {
            (_, _, Err(error)) => format!("{error:#}"),
            (None, ..) => {
                "The scene sets its own sky and post-processing. Make lighting settings to share them.".into()
            }
            (Some(_), None, _) => "Built-in lighting. Make a copy to change it.".into(),
            (Some(_), Some(_), _) => String::new(),
        };
        ui.set_visible(self.note, !note.is_empty());
        ui.set_text(self.note, &note);
        ui.set_visible(self.copy, file.is_none());
        ui.set_text(self.copy, if current.is_none() { "New Lighting Settings" } else { "Make a Copy" });
        let values = read.and_then(|file| effective(&file)).unwrap_or_default();
        self.fields.show(ui, file, values);
    }

    /// What was changed on the page this frame.
    pub fn update(&mut self, ui: &mut Ui) -> Option<LightingAction> {
        if ui.changed(self.scene) {
            let chosen = self.choices.get(ui.selected_option(self.scene)).cloned().flatten();
            return Some(LightingAction::SetScene(chosen));
        }
        if ui.clicked(self.copy) {
            return Some(LightingAction::Copy);
        }
        if ui.changed(self.look) {
            let chosen = self.looks.get(ui.selected_option(self.look)).cloned().flatten();
            return Some(LightingAction::Wear(chosen));
        }
        if ui.changed(self.textures) {
            let chosen = self.packs.get(ui.selected_option(self.textures)).cloned().flatten();
            return Some(LightingAction::WearTextures(chosen));
        }
        self.fields.update(ui)
    }

    /// The menu the texture pack the scene is shown under is picked from.
    pub fn texture_menu(&self) -> NodeId {
        self.textures
    }

    /// The menu the look pack the scene is shown under is picked from.
    pub fn look_menu(&self) -> NodeId {
        self.look
    }

    /// The colour setting whose swatch was clicked this frame.
    pub fn color_clicked(&self, ui: &Ui) -> Option<ColorClicked> {
        self.fields.color_clicked(ui)
    }

    /// The setting a page shows, by its title, for tests and the palette.
    pub fn row_of(&self, title: &str) -> Option<NodeId> {
        self.fields.row_of(title)
    }

    /// The lighting settings shown, as the scene names them.
    pub fn shown(&self) -> Option<&str> {
        self.shown.as_deref()
    }
}

/// A colour setting's swatch clicked: the file and setting it is, the
/// swatch, and its colour.
pub struct ColorClicked {
    pub file: PathBuf,
    pub path: &'static [&'static str],
    pub swatch: NodeId,
    pub srgb: [f32; 3],
}

/// Every setting of lighting settings, a tab for each group, as the Lighting
/// page and a `.lighting` asset's Inspector show them; changed in place
/// when they are a file of the project's.
pub struct LightingFields {
    tabs: Vec<(&'static str, NodeId)>,
    headings: Vec<(&'static str, NodeId)>,
    group: &'static str,
    rows: Vec<Row>,
    /// The file shown, when it is the project's to change.
    file: Option<PathBuf>,
    /// Every setting as it is in effect; for a look pack, only what the
    /// pack writes.
    values: toml::Table,
    /// Whether these are a look pack's settings: a row is empty where the
    /// pack leaves a setting to the level.
    pack: bool,
}

impl LightingFields {
    /// A tab for each group into `tab_bar` and every setting into `view`.
    pub fn new(ui: &mut Ui, tab_bar: NodeId, view: NodeId) -> Self {
        Self::build(ui, tab_bar, view, false)
    }

    /// A look pack's settings: its description, the settings lighting has
    /// but for the weather, which is the level's, and how blocks are drawn.
    pub fn for_pack(ui: &mut Ui, tab_bar: NodeId, view: NodeId) -> Self {
        Self::build(ui, tab_bar, view, true)
    }

    fn build(ui: &mut Ui, tab_bar: NodeId, view: NodeId, pack: bool) -> Self {
        let (first, last) = PACK_SETTINGS.split_at(1);
        let lighting = SETTINGS.iter().filter(|info| !pack || info.group != "Weather");
        let infos: Vec<&'static Info> = match pack {
            true => first.iter().chain(lighting).chain(last).collect(),
            false => lighting.collect(),
        };
        let mut tabs: Vec<(&'static str, NodeId)> = Vec::new();
        let body = ui.panel(view, Style::column());
        ui.add_class(body, "settings-body");
        let mut headings: Vec<(&'static str, NodeId)> = Vec::new();
        let mut rows = Vec::new();
        for info in infos {
            if !tabs.iter().any(|(group, _)| *group == info.group) {
                let tab = ui.button(tab_bar, info.group);
                ui.add_class(tab, "settings-tab");
                ui.set_tooltip(tab, group_help(info.group), None);
                tabs.push((info.group, tab));
                let heading = ui.add_label(body, info.group);
                ui.add_class(heading, "settings-group");
                headings.push((info.group, heading));
            }
            let row = ui.panel(body, Style::default());
            ui.add_class(row, "prop");
            ui.add_class(row, "setting");
            let label = ui.add_label(row, info.title);
            ui.add_class(label, "prop-label");
            ui.add_class(label, "scrub");
            ui.set_tooltip(label, info.help, None);
            let control = control(ui, row, info.kind, pack);
            // A colour has no empty state to pick, so a pack's has a button that gives it back to the level.
            let reset = (pack && matches!(info.kind, Kind::Color)).then(|| {
                let reset = ui.button(row, "×");
                ui.add_class(reset, "setting-reset");
                ui.set_tooltip(reset, "Give this colour back to the level", None);
                reset
            });
            rows.push(Row { info, row, label, control, reset, pack });
        }
        let group = rows[0].info.group;
        let mut fields = Self { tabs, headings, group, rows, file: None, values: toml::Table::new(), pack };
        fields.show_group(ui);
        fields
    }

    /// Shows lighting settings' values, in effect, changeable when `file` is
    /// the project's file of them.
    pub fn show(&mut self, ui: &mut Ui, file: Option<PathBuf>, values: toml::Table) {
        self.file = file;
        self.values = values;
        let editable = self.file.is_some();
        for row in &self.rows {
            ui.set_enabled(row.control.node(), editable);
            let value = find(&self.values, row.info.path);
            show(ui, row, value);
            if !row.pack {
                continue;
            }
            // What the pack writes stands out from what it leaves to the level.
            match value.is_some() {
                true => ui.add_class(row.row, "written"),
                false => ui.remove_class(row.row, "written"),
            }
            if let Some(reset) = row.reset {
                ui.set_visible(reset, value.is_some() && editable);
            }
        }
        self.show_group(ui);
    }

    /// Shows the project's `.lookpack` file at `file`: only what it writes,
    /// every other row empty, as the level's own.
    fn show_pack_file(&mut self, ui: &mut Ui, file: &Path) {
        let read = std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display())).and_then(|text| {
            voxear_engine::look_packs::LookPackFile::parse(&text).with_context(|| format!("reading {}", file.display()))?;
            toml::from_str::<toml::Table>(&text).with_context(|| format!("reading {}", file.display()))
        });
        match read {
            Ok(written) => self.show(ui, Some(file.to_path_buf()), written),
            Err(error) => {
                tracing::error!("{error:#}");
                self.show(ui, None, toml::Table::new());
            }
        }
    }

    /// Shows the project's `.lighting` file at `file`, or for a look pack's
    /// settings, its `.lookpack` file.
    pub fn show_file(&mut self, ui: &mut Ui, file: &Path) {
        if self.pack {
            return self.show_pack_file(ui, file);
        }
        let read = std::fs::read_to_string(file)
            .with_context(|| format!("reading {}", file.display()))
            .and_then(|text| toml::from_str::<LightingFile>(&text).with_context(|| format!("reading {}", file.display())));
        match read.and_then(|lighting| effective(&lighting)) {
            Ok(values) => self.show(ui, Some(file.to_path_buf()), values),
            Err(error) => {
                tracing::error!("{error:#}");
                self.show(ui, None, toml::Table::new());
            }
        }
    }

    /// A setting changed this frame, as a change to make to the file.
    pub fn update(&mut self, ui: &mut Ui) -> Option<LightingAction> {
        if let Some(&(group, _)) = self.tabs.iter().find(|(_, tab)| ui.clicked(*tab)) {
            self.group = group;
            self.show_group(ui);
        }
        let file = self.file.clone()?;
        self.rows.iter().find_map(|row| {
            let reset = row.reset.is_some_and(|reset| ui.clicked(reset));
            let value = if reset { None } else { read_control(ui, row)? };
            Some(LightingAction::Set { file: file.clone(), path: row.info.path, value })
        })
    }

    /// The colour setting whose swatch was clicked this frame.
    pub fn color_clicked(&self, ui: &Ui) -> Option<ColorClicked> {
        let file = self.file.clone()?;
        self.rows.iter().find_map(|row| match row.control {
            Control::Color(swatch) if ui.clicked(swatch) => {
                let srgb = find(&self.values, row.info.path).and_then(rgb).unwrap_or([1.0; 3]);
                Some(ColorClicked { file: file.clone(), path: row.info.path, swatch, srgb })
            }
            _ => None,
        })
    }

    pub fn row_of(&self, title: &str) -> Option<NodeId> {
        self.rows.iter().find(|row| row.info.title == title).map(|row| row.row)
    }

    /// The tab's group of settings, each only while it does anything.
    fn show_group(&mut self, ui: &mut Ui) {
        for (group, tab) in &self.tabs {
            ui.set_checked(*tab, *group == self.group);
        }
        for (group, heading) in &self.headings {
            ui.set_visible(*heading, *group == self.group);
        }
        // A pack shows every setting: it may write one that does nothing until the level turns its part on.
        for row in &self.rows {
            let does = self.pack || applies(&self.values, row.info.when);
            ui.set_visible(row.row, row.info.group == self.group && does);
        }
    }
}

/// What a group of settings is for, shown when the pointer rests on its tab.
fn group_help(group: &str) -> &'static str {
    match group {
        "Sky" => "The sky, and the light it gives everything from all round.",
        "Clouds" => "The clouds in the sky, and the shadows they cast as they drift.",
        "Weather" => "The weather laid over this lighting: a kind, or amounts of your own. Scripts can turn it too.",
        "Sun" => "How the sun looks: its colour, brightness and shadows. Where it shines from is the sun Obj's rotation.",
        "Exposure" => "How bright the picture is, and how the camera adjusts to light and dark, as an eye does.",
        "Colour" => "The colour grade: contrast, saturation, warmth and lens effects over the finished picture.",
        "Bloom" => "The soft glow round bright lights and highlights.",
        "Fog" => "Mist and dust in the air, and the light shafts that show in it.",
        "Shading" => "How light settles in the scene: bounced light, shade in corners, small shadows and reflections.",
        "Lens" => "Camera effects: focus blur and motion blur.",
        "Pack" => "What the pack says of itself. In the other tabs, an empty row is the level's own setting.",
        "Blocks" => "How every Block World's faces are drawn under the pack.",
        "Water" => "How water looks under the pack, wherever it is: the sea, Pools and the water of Block Worlds.",
        _ => "",
    }
}

/// Whether every condition holds in `values`, as a file writes them.
fn applies(values: &toml::Table, when: When) -> bool {
    when.iter().all(|(path, wanted)| match find(values, path) {
        Some(toml::Value::Boolean(on)) => on.to_string() == *wanted,
        Some(toml::Value::String(text)) => text == wanted,
        _ => false,
    })
}

/// The change to make for a colour picked for a setting.
pub fn set_color(file: PathBuf, path: &'static [&'static str], srgb: [f32; 3]) -> LightingAction {
    LightingAction::Set { file, path, value: Some(color_value(srgb)) }
}

/// The project's file of lighting settings a scene names, when it is one.
fn project_file(game: &Game, reference: &str) -> Option<PathBuf> {
    let AssetRef::Asset { guid, .. } = game.content.parse(reference).ok()? else { return None };
    let entry = game.content.db().get(guid)?;
    Some(game.project.root.join(&entry.path))
}

/// Lighting settings as written: a built-in's look, or a project file's text.
fn read(game: &Game, reference: &str) -> Result<LightingFile> {
    match game.content.parse(reference)? {
        AssetRef::Builtin(name) => LightingFile::builtin(&name),
        AssetRef::Asset { .. } => {
            let file = project_file(game, reference).with_context(|| format!("no lighting settings {reference}"))?;
            let text = std::fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?;
            toml::from_str(&text).with_context(|| format!("reading {reference}"))
        }
    }
}

/// Every setting of `file` as it is in effect, laid out as a `.lighting`
/// file writes them: the look's where the file writes none.
fn effective(file: &LightingFile) -> Result<toml::Table> {
    let lighting = file.resolve()?;
    let mut table = toml::Table::new();
    table.insert("look".into(), toml::Value::String(file.look.clone().unwrap_or_default()));
    let sky = sky_data(&lighting.sky);
    let color = |c: [f32; 3]| toml::Value::Array(c.iter().map(|&v| toml::Value::Float(f64::from(v))).collect());
    let mut written: toml::Table = toml::Table::try_from(sky)?;
    // A sky file leaves out settings at their defaults; the page shows every one.
    for (key, value) in [
        ("stars", toml::Value::Float(f64::from(sky.stars))),
        ("haze", toml::Value::Float(f64::from(sky.haze))),
        ("haze_tint", color(sky.haze_tint)),
        ("haze_glow", toml::Value::Float(f64::from(sky.haze_glow))),
        ("distance_haze", toml::Value::Float(f64::from(sky.distance_haze))),
        ("sun_size", toml::Value::Float(f64::from(sky.sun_size))),
        ("clouds", toml::Value::try_from(sky.clouds)?),
        (
            "mode",
            toml::Value::String(if sky.mode == SkyModeData::Atmosphere { "atmosphere" } else { "gradient" }.into()),
        ),
    ] {
        written.insert(key.into(), value);
    }
    table.insert("sky".into(), toml::Value::Table(written));
    let own = file.sun.unwrap_or_default();
    let mut sun = toml::Table::new();
    if let Some(kelvin) = own.temperature {
        sun.insert("temperature".into(), toml::Value::Float(f64::from(kelvin)));
    }
    sun.insert("tint".into(), color(own.tint.unwrap_or([1.0; 3])));
    sun.insert("strength".into(), toml::Value::Float(f64::from(lighting.sun.strength)));
    sun.insert("softness".into(), toml::Value::Float(f64::from(lighting.sun.softness)));
    sun.insert("shadow_tint".into(), color(voxear_engine::srgb(lighting.sun.shadow_tint.to_array())));
    sun.insert("shadow_distance".into(), toml::Value::Float(f64::from(lighting.sun.shadow_distance)));
    sun.insert("shadow_split".into(), toml::Value::Float(f64::from(lighting.sun.shadow_split)));
    table.insert("sun".into(), toml::Value::Table(sun));
    let post = PostProcess::from_toml(&lighting.post)?;
    table.insert("post".into(), toml::Value::Table(toml::Table::try_from(post)?));
    table.insert("weather".into(), toml::Value::try_from(lighting.weather)?);
    Ok(table)
}

/// What new lighting settings start with.
const COPY_HEADER: &str = "# Voxear lighting settings. A scene names them, lighting = \"<this file>\",\n\
                           # and scripts apply them with Lighting.Apply.\n";

/// Text for new lighting settings holding a scene's own sky and
/// post-processing, so they carry over into them.
pub fn own_text(own: &LightingFile) -> Result<String> {
    let mut table = toml::Table::try_from(own)?;
    tidy(&mut table);
    Ok(format!("{COPY_HEADER}{}", toml::to_string(&table)?))
}

/// A scene's own sky and post-processing as lighting settings: the look its
/// `[post]` picks, which brings its sky and sun, its `[sky]`, and the rest
/// of its `[post]`.
pub fn own_lighting(sky: Option<SkyData>, post: Option<&toml::Table>) -> Result<LightingFile> {
    let mut file = LightingFile { format: voxear_engine::lighting::LIGHTING_FORMAT, ..LightingFile::default() };
    let mut post = post.cloned().unwrap_or_default();
    file.look = post.remove("look").and_then(|look| look.as_str().map(str::to_string));
    file.post = (!post.is_empty()).then_some(post);
    file.sky = sky.map(|sky| toml::Table::try_from(sky)).transpose()?;
    Ok(file)
}

/// The lighting settings the scene at `path` names, as the editor holds
/// them while it is open, and its own sky and post-processing, from its
/// file.
pub fn scene_lighting(game: &Game, path: &str) -> Result<(Option<String>, LightingFile)> {
    let scene = scene_file::read(&game.content.db().file(path))?;
    let current = match game.levels.lighting_of(path) {
        Some(open) => open.map(str::to_string),
        None => scene.lighting.clone(),
    };
    Ok((current, own_lighting(scene.sky, scene.post.as_ref())?))
}

/// The sky and post-processing the level has from its scenes, as lighting
/// settings: the open scene's own when it names none.
fn world_own(game: &Game) -> Result<LightingFile> {
    let world = &game.engine.world;
    let post = world.resource::<PostTable>().map(|table| &table.0);
    let sky = world.resource::<Sky>().copied().unwrap_or_default();
    let sky = world.resource::<SkyFromLook>().is_none().then(|| sky_data(&sky));
    own_lighting(sky, post)
}

/// Rounds every number in `table` to four decimals, so a file written from
/// settings held in single precision reads `0.506`, not `0.5059999823570251`.
fn tidy(table: &mut toml::Table) {
    fn value(v: &mut toml::Value) {
        match v {
            toml::Value::Float(f) => *f = (*f * 10_000.0).round() / 10_000.0,
            toml::Value::Array(items) => items.iter_mut().for_each(value),
            toml::Value::Table(inner) => tidy(inner),
            _ => {}
        }
    }
    table.iter_mut().for_each(|(_, v)| value(v));
}

/// The value at `path` in `table`.
fn find<'a>(table: &'a toml::Table, path: &[&str]) -> Option<&'a toml::Value> {
    let (last, tables) = path.split_last()?;
    let mut at = table;
    for key in tables {
        at = at.get(*key)?.as_table()?;
    }
    at.get(*last)
}

fn rgb(value: &toml::Value) -> Option<[f32; 3]> {
    let array = value.as_array()?;
    let channel = |i: usize| array.get(i).and_then(|v| v.as_float().or_else(|| v.as_integer().map(|n| n as f64)));
    Some([channel(0)? as f32, channel(1)? as f32, channel(2)? as f32])
}

fn color_value(srgb: [f32; 3]) -> toml_edit::Value {
    let rounded = srgb.map(|c| f64::from((c.clamp(0.0, 1.0) * 1000.0).round() / 1000.0));
    toml_edit::Value::Array(rounded.into_iter().collect())
}

fn control(ui: &mut Ui, row: NodeId, kind: Kind, pack: bool) -> Control {
    match kind {
        Kind::Toggle if pack => {
            let dropdown = ui.dropdown(row, &[LEVELS_OWN, "On", "Off"], 0);
            ui.add_class(dropdown, "value-choice");
            Control::Tri(dropdown)
        }
        Kind::Toggle => {
            let toggle = ui.toggle(row, "", false);
            ui.add_class(toggle, "checkbox");
            Control::Toggle(toggle)
        }
        Kind::Choice(options) => {
            let own = pack.then_some(LEVELS_OWN);
            let labels: Vec<&str> = own.into_iter().chain(options.iter().map(|(label, _)| *label)).collect();
            let dropdown = ui.dropdown(row, &labels, 0);
            ui.add_class(dropdown, "value-choice");
            Control::Choice(dropdown)
        }
        Kind::Look => {
            let none = if pack { LEVELS_OWN } else { "None" };
            let labels: Vec<&str> = std::iter::once(none).chain(LookName::ALL.iter().map(|(.., title)| *title)).collect();
            let dropdown = ui.dropdown(row, &labels, 0);
            ui.add_class(dropdown, "value-choice");
            Control::Choice(dropdown)
        }
        Kind::Number { unit, .. } => {
            let holder = ui.panel(row, Style::row());
            ui.add_class(holder, "prop-value");
            let field = ui.text_field(holder, "");
            ui.add_class(field, "value-input");
            ui.add_class(field, "number-input");
            let unit = ui.add_label(holder, unit);
            ui.add_class(unit, "setting-unit");
            Control::Number(field)
        }
        Kind::Color => {
            let swatch = ui.panel(row, Style::default());
            ui.add_class(swatch, "swatch");
            Control::Color(swatch)
        }
        Kind::Whole { .. } => {
            let field = ui.text_field(row, "");
            ui.add_class(field, "value-input");
            ui.add_class(field, "number-input");
            Control::Number(field)
        }
        Kind::Text => {
            let field = ui.text_field(row, "");
            ui.add_class(field, "value-input");
            Control::Text(field)
        }
    }
}

/// A change made in a row's control this frame: the value to write, or
/// `None` inside to take the setting out of the file.
fn read_control(ui: &Ui, row: &Row) -> Option<Option<toml_edit::Value>> {
    match (row.control, row.info.kind) {
        (Control::Toggle(toggle), _) if ui.changed(toggle) => Some(Some(ui.is_checked(toggle).into())),
        // A pack's menus start with the level's own, which takes the setting out of the file.
        (Control::Tri(dropdown), _) if ui.changed(dropdown) => match ui.selected_option(dropdown) {
            0 => Some(None),
            index => Some(Some((index == 1).into())),
        },
        (Control::Choice(dropdown), Kind::Choice(options)) if ui.changed(dropdown) => {
            let Some(index) = ui.selected_option(dropdown).checked_sub(usize::from(row.pack)) else {
                return Some(None);
            };
            let (_, value) = options.get(index)?;
            Some((!value.is_empty()).then(|| (*value).into()))
        }
        (Control::Number(field), Kind::Whole { max }) if ui.changed(field) => {
            let text = ui.text(field);
            let text = text.trim();
            if text.is_empty() {
                return Some(None);
            }
            let whole = text.parse::<f64>().ok()?.round().clamp(0.0, f64::from(max)) as i64;
            Some(Some(whole.into()))
        }
        (Control::Text(field), _) if ui.changed(field) => {
            let text = ui.text(field);
            let text = text.trim();
            Some((!text.is_empty()).then(|| text.into()))
        }
        // None, the first, takes the look out of the file.
        (Control::Choice(dropdown), Kind::Look) if ui.changed(dropdown) => match ui.selected_option(dropdown) {
            0 => Some(None),
            index => LookName::ALL.get(index - 1).map(|(_, key, _)| Some((*key).into())),
        },
        (Control::Number(field), Kind::Number { min, max, .. }) if ui.changed(field) => {
            let text = ui.text(field);
            let text = text.trim();
            if text.is_empty() {
                return Some(None);
            }
            let number = text.parse::<f64>().ok()?.clamp(f64::from(min), f64::from(max));
            Some(Some(((number * 10_000.0).round() / 10_000.0).into()))
        }
        _ => None,
    }
}

/// Shows a setting's value in its control, except in a field being typed in.
fn show(ui: &mut Ui, row: &Row, value: Option<&toml::Value>) {
    match (row.control, row.info.kind) {
        (Control::Toggle(toggle), _) => {
            let on = value.and_then(toml::Value::as_bool).unwrap_or(false);
            if ui.is_checked(toggle) != on {
                ui.set_checked(toggle, on);
            }
        }
        (Control::Tri(dropdown), _) => {
            let index = match value.and_then(toml::Value::as_bool) {
                None => 0,
                Some(true) => 1,
                Some(false) => 2,
            };
            if ui.selected_option(dropdown) != index {
                ui.set_selected_option(dropdown, index);
            }
        }
        (Control::Text(field), _) => {
            let text = value.and_then(toml::Value::as_str).unwrap_or("");
            if ui.focused() != Some(field) && ui.text(field) != text {
                ui.set_text(field, text);
            }
        }
        (Control::Choice(dropdown), Kind::Choice(options)) => {
            let written = value.and_then(toml::Value::as_str).unwrap_or("");
            let listed = options.iter().position(|(_, v)| *v == written);
            // A pack's menu starts with the level's own, for a setting it does not write.
            let index = match row.pack {
                true => listed.filter(|_| value.is_some()).map_or(0, |at| at + 1),
                false => listed.unwrap_or(0),
            };
            if ui.selected_option(dropdown) != index {
                ui.set_selected_option(dropdown, index);
            }
        }
        (Control::Choice(dropdown), Kind::Look) => {
            let written = value.and_then(toml::Value::as_str).unwrap_or("");
            let index = LookName::ALL.iter().position(|(_, key, _)| *key == written).map_or(0, |at| at + 1);
            if ui.selected_option(dropdown) != index {
                ui.set_selected_option(dropdown, index);
            }
        }
        (Control::Number(field), _) => {
            let number = value.and_then(|v| v.as_float().or_else(|| v.as_integer().map(|n| n as f64)));
            let text = number.map_or_else(String::new, format_number);
            if ui.focused() != Some(field) && ui.text(field) != text {
                ui.set_text(field, &text);
            }
        }
        (Control::Color(swatch), _) => {
            let [r, g, b] = value.and_then(rgb).unwrap_or([0.5; 3]).map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u32);
            let color = Color::hex(r << 16 | g << 8 | b);
            if ui.style(swatch).and_then(|s| s.background) != Some(color) {
                ui.set_inline_style(swatch, &format!("background-color: {}", color.css()));
            }
        }
        _ => {}
    }
}

/// A number as short as it can be written: up to four decimals.
fn format_number(value: f64) -> String {
    let text = format!("{value:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Writes one setting into a `.lighting` file, or takes it out, keeping
/// everything else in the file as written, comments and all.
pub fn write_setting(file: &Path, path: &[&str], value: Option<toml_edit::Value>) -> Result<()> {
    let text = std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
    let mut document: toml_edit::DocumentMut = text.parse().with_context(|| format!("reading {}", file.display()))?;
    let (last, tables) = path.split_last().context("no setting")?;
    let mut table = document.as_table_mut();
    for key in tables {
        let entry = table.entry(key).or_insert_with(|| {
            let mut fresh = toml_edit::Table::new();
            fresh.set_implicit(true);
            toml_edit::Item::Table(fresh)
        });
        table = entry.as_table_mut().with_context(|| format!("`{key}` in {} is not a table", file.display()))?;
    }
    let emptied = value.is_none();
    match value {
        Some(value) => drop(table.insert(last, toml_edit::Item::Value(value))),
        None => drop(table.remove(last)),
    }
    // A table left with nothing in it goes too: an empty `[blocks]` would say something of its own.
    if emptied {
        prune(document.as_table_mut(), tables);
    }
    std::fs::write(file, document.to_string()).with_context(|| format!("writing {}", file.display()))
}

/// Takes the table at `path` under `table` out if nothing is left in it,
/// and then each table over it that leaves empty.
fn prune(table: &mut toml_edit::Table, path: &[&str]) {
    let Some((first, rest)) = path.split_first() else { return };
    let Some(inner) = table.get_mut(first).and_then(toml_edit::Item::as_table_mut) else { return };
    prune(inner, rest);
    if inner.is_empty() {
        table.remove(first);
    }
}

/// Text for a copy of lighting settings: a built-in's look, or the file's
/// own text as it is; or with none, the scene's own sky and
/// post-processing, so they carry over into the new lighting.
pub fn copy_text(game: &Game, reference: Option<&str>) -> Result<String> {
    let Some(reference) = reference else { return own_text(&world_own(game)?) };
    match game.content.parse(reference)? {
        AssetRef::Builtin(name) => Ok(format!("{COPY_HEADER}format = 1\nlook = \"{name}\"\n")),
        AssetRef::Asset { .. } => {
            let file = project_file(game, reference).with_context(|| format!("no lighting settings {reference}"))?;
            Ok(std::fs::read_to_string(&file)?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_setting_is_written_into_its_table_keeping_the_rest_and_taken_out_again() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Dusk.lighting");
        std::fs::write(&file, "# Dusk\nformat = 1\nlook = \"golden_hour\"\n\n[sky]\nhaze = 2.0\n").unwrap();
        write_setting(&file, &["post", "bloom", "intensity"], Some(0.25.into())).unwrap();
        write_setting(&file, &["sky", "haze_glow"], Some(0.9.into())).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.starts_with("# Dusk\n"), "comments stay: {text}");
        let read: LightingFile = toml::from_str(&text).unwrap();
        assert_eq!(read.post.as_ref().unwrap()["bloom"]["intensity"].as_float(), Some(0.25));
        assert_eq!(read.sky.as_ref().unwrap()["haze"].as_float(), Some(2.0));
        assert_eq!(read.sky.as_ref().unwrap()["haze_glow"].as_float(), Some(0.9));
        write_setting(&file, &["sky", "haze"], None).unwrap();
        let read: LightingFile = toml::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert!(read.sky.unwrap().get("haze").is_none(), "back to the look's");
    }

    #[test]
    fn a_table_left_empty_goes_with_its_last_setting_and_others_stay() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Cosy.lookpack");
        std::fs::write(&file, "# Cosy\ndescription = \"Warm.\"\n\n[post.bloom]\nintensity = 0.3\n").unwrap();
        write_setting(&file, &["blocks", "voxels"], Some(8.into())).unwrap();
        write_setting(&file, &["post", "vignette", "intensity"], Some(0.2.into())).unwrap();
        let pack = voxear_engine::look_packs::LookPackFile::parse(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(pack.blocks.map(|blocks| blocks.voxels), Some(8));
        // Voxels emptied: no [blocks] is left to say plain faces, and the pack leaves blocks to each world.
        write_setting(&file, &["blocks", "voxels"], None).unwrap();
        write_setting(&file, &["post", "vignette", "intensity"], None).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(!text.contains("blocks") && !text.contains("vignette"), "{text}");
        let pack = voxear_engine::look_packs::LookPackFile::parse(&text).unwrap();
        assert!(pack.blocks.is_none() && pack.description == "Warm.");
        assert_eq!(pack.post.as_ref().unwrap()["bloom"]["intensity"].as_float(), Some(0.3), "{text}");
        assert!(text.starts_with("# Cosy\n"), "comments stay: {text}");
        // The last of them all: the pack changes nothing, and still reads.
        write_setting(&file, &["post", "bloom", "intensity"], None).unwrap();
        let pack = voxear_engine::look_packs::LookPackFile::parse(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert!(pack.post.is_none());
    }

    #[test]
    fn a_look_packs_rows_are_lightings_without_the_weather_with_its_own_and_each_says_what_it_does() {
        assert!(PACK_SETTINGS.iter().all(|info| info.help.len() > 20 && !group_help(info.group).is_empty()));
        assert_eq!(PACK_SETTINGS[0].path, ["description"], "its description comes first");
        // What packs most often write has a row: the sun's shade, the glow, the clouds' kind, and the pack's own.
        let rows = || SETTINGS.iter().filter(|info| info.group != "Weather").chain(PACK_SETTINGS);
        let wanted: [&[&str]; 5] = [
            &["sun", "shadow_tint"],
            &["post", "bloom", "intensity"],
            &["sky", "clouds", "kind"],
            &["blocks", "voxels"],
            &["water", "deep"],
        ];
        for path in wanted {
            assert!(rows().any(|info| info.path == path), "no row for {path:?}");
        }
        // The weather is the level's: a pack's file refuses it, so it has no rows for it.
        assert!(rows().all(|info| info.path[0] != "weather"));
        assert!(voxear_engine::look_packs::LookPackFile::parse("[weather]\nkind = \"rain\"").is_err());
    }

    #[test]
    fn a_clouds_setting_is_written_under_the_skys_clouds_and_changes_only_itself() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Summer.lighting");
        std::fs::write(&file, "format = 1\nlook = \"highland_summer\"\n").unwrap();
        write_setting(&file, &["sky", "clouds", "cover"], Some(0.75.into())).unwrap();
        let read: LightingFile = toml::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        let clouds = read.resolve().unwrap().sky.clouds;
        let look = LightingFile::builtin("highland_summer").unwrap().resolve().unwrap().sky.clouds;
        assert_eq!((clouds.cover, clouds.height, clouds.drift), (0.75, look.height, look.drift));
        let shown = effective(&read).unwrap();
        assert_eq!(find(&shown, &["sky", "clouds", "cover"]).and_then(|v| v.as_float()), Some(0.75));
        // A look without clouds still shows every setting of theirs, at none.
        let indoors = effective(&LightingFile::builtin("candlelit_castle").unwrap()).unwrap();
        assert_eq!(find(&indoors, &["sky", "clouds", "cover"]).and_then(|v| v.as_float()), Some(0.0));
        assert!(find(&indoors, &["sky", "clouds", "height"]).is_some());
    }

    #[test]
    fn the_weather_is_written_as_a_kind_and_its_own_amounts_show_only_for_custom() {
        use voxear_engine::weather::WeatherKind;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Moor.lighting");
        std::fs::write(&file, "format = 1\nlook = \"highland_summer\"\n").unwrap();
        let read = |file: &Path| toml::from_str::<LightingFile>(&std::fs::read_to_string(file).unwrap()).unwrap();
        let shown = effective(&read(&file)).unwrap();
        assert_eq!(find(&shown, &["weather", "kind"]).and_then(|v| v.as_str()), Some("clear"), "clear unless set");
        let amounts = SETTINGS.iter().filter(|info| info.group == "Weather" && info.path != ["weather", "kind"]);
        assert!(amounts.clone().count() == 8 && amounts.clone().all(|info| !applies(&shown, info.when)));
        write_setting(&file, &["weather", "kind"], Some("storm".into())).unwrap();
        assert_eq!(read(&file).resolve().unwrap().weather.kind, WeatherKind::Storm);
        // Custom shows its amounts, and one written is the weather's.
        write_setting(&file, &["weather", "kind"], Some("custom".into())).unwrap();
        write_setting(&file, &["weather", "snow"], Some(0.4.into())).unwrap();
        let lighting = read(&file);
        assert_eq!(lighting.resolve().unwrap().weather.weather().now().snow, 0.4);
        let shown = effective(&lighting).unwrap();
        assert!(amounts.clone().all(|info| applies(&shown, info.when) && find(&shown, info.path).is_some()));
    }

    #[test]
    fn every_setting_and_group_says_what_it_does() {
        for info in SETTINGS {
            assert!(info.help.len() > 20, "{} explains itself", info.title);
            assert!(!group_help(info.group).is_empty(), "the {} tab explains itself", info.group);
        }
    }

    #[test]
    fn every_setting_shows_a_value_in_effect() {
        let file = LightingFile::builtin("golden_hour").unwrap();
        let values = effective(&file).unwrap();
        for info in SETTINGS {
            let optional = info.path == ["sun", "temperature"];
            assert!(optional || find(&values, info.path).is_some(), "{:?} has a value", info.path);
        }
        assert_eq!(find(&values, &["sky", "mode"]).and_then(|v| v.as_str()), Some("atmosphere"));
    }
}
