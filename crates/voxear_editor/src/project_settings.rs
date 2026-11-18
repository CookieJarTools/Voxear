//! What Project Settings shows: every setting kept in the project's
//! `voxear.toml`, by category and section, with its label and tooltip (see
//! `setting_info`), and how each is read from the project and written back.
//! A setting's key is its table and key in the file (`audio.reverb_mode`),
//! but for those Project Settings keeps in its own way: `modules.*`, a
//! module each; `world.*`, the block worlds' budget, which a preset sets
//! whole and which this machine may keep for itself (`world.machine`); and
//! the parts of the window of their own, the layers, the collision matrix
//! and the languages' fonts. The window itself is `settings_window`; the
//! editor puts changes into effect in `editor::modules`.

use std::{collections::HashMap, sync::LazyLock};

use serde_json::{Value as Json, json};
use voxear_assets::{AssetDb, AssetKind};
use voxear_engine::{Game, Project, modules::Module};
use voxear_terrain::{MachineBudget, WorldBudget, WorldQuality};

use crate::setting_info::{Category, Kind, SectionHelp, Setting, kept_word, shown_label};

/// The categories down the window's side, in order.
pub const CATEGORIES: &[Category] = &[
    Category {
        name: "Audio",
        about: "How 3D sound is heard: headphones, room reverb and ray-traced sound",
        manual: "Getting Started/Project Settings",
    },
    Category {
        name: "Content",
        about: "How builds pack the project's assets into bundles",
        manual: "Content and Builds/What Goes into a Build",
    },
    Category {
        name: "Foliage",
        about: "Plants in the wind, and pushed aside by what walks through them",
        manual: "Models and Materials/Wind",
    },
    Category {
        name: "Graphics",
        about: "What drawing through Portals and trails on water may spend",
        manual: "Objs and Levels/Portals",
    },
    Category { name: "Input", about: "The input actions the game plays with", manual: "Scripting/Reading Input" },
    Category {
        name: "Localization",
        about: "The languages the game is in, and their fonts",
        manual: "Languages/Translating Your Game",
    },
    Category {
        name: "Modules",
        about: "The engine's optional systems the project has",
        manual: "Getting Started/Modules",
    },
    Category {
        name: "Network",
        about: "Ticks, ports, players and bandwidth for networked games",
        manual: "Networking/Playing over the Network",
    },
    Category {
        name: "Physics",
        about: "Gravity, and which layers collide",
        manual: "Physics/Colliders and Rigidbodies",
    },
    Category {
        name: "Navigation",
        about: "What each kind of ground costs things finding their way",
        manual: "Navigation/Areas and Costs",
    },
    Category { name: "Physics 2D", about: "Gravity for 2D physics", manual: "Physics/2D Physics" },
    Category {
        name: "Player",
        about: "The game's name and the level it opens on",
        manual: "Getting Started/Making a Build",
    },
    Category {
        name: "Quality",
        about: "What making block worlds and particles may spend",
        manual: "Terrain/Block Worlds",
    },
    Category { name: "Scripts", about: "The game's C# project", manual: "Scripting/Your First Script" },
    Category { name: "Tags and Layers", about: "The project's 32 layers", manual: "Getting Started/Project Settings" },
    Category {
        name: "Time",
        about: "How long fixed steps and frames may be",
        manual: "Getting Started/Project Settings",
    },
    Category {
        name: "World",
        about: "The floating origin for large worlds",
        manual: "Objs and Levels/Floating Origin",
    },
];

/// Lines of help under sections' titles.
pub const HELP: &[SectionHelp] = &[
    SectionHelp {
        category: "Audio",
        section: "Ray-Traced Sound",
        help: "Sound heard through the scene the GPU traces for light, blocks and terrain too. A level may set its own.",
    },
    SectionHelp {
        category: "Graphics",
        section: "Water Trails",
        help: "Ripples and foam behind what moves on water: swimmers, boats, anything with a body. A Water Wake tunes an Obj's own.",
    },
    SectionHelp {
        category: "Network",
        section: "Session",
        help: "Used when the game hosts, serves or joins; taken up the next time a session starts.",
    },
    SectionHelp {
        category: "Player",
        section: "Builds",
        help: "Window size, version, company and defines are set for each build profile, in the Build window.",
    },
    SectionHelp {
        category: "Quality",
        section: "Block Worlds",
        help: "A preset sets every number under it; changing one makes it Custom.",
    },
    SectionHelp {
        category: "World",
        section: "Floating Origin",
        help: "Keeps positions exact far out in large worlds, by moving the world back round the camera.",
    },
];

/// The languages a project may list, each as shown and by its code.
pub const LANGUAGES: &[(&str, &str)] = &[
    ("English", "en"),
    ("English (US)", "en-US"),
    ("English (UK)", "en-GB"),
    ("French", "fr"),
    ("German", "de"),
    ("Spanish", "es"),
    ("Spanish (Latin America)", "es-419"),
    ("Italian", "it"),
    ("Portuguese", "pt"),
    ("Portuguese (Brazil)", "pt-BR"),
    ("Dutch", "nl"),
    ("Polish", "pl"),
    ("Czech", "cs"),
    ("Hungarian", "hu"),
    ("Romanian", "ro"),
    ("Swedish", "sv"),
    ("Danish", "da"),
    ("Norwegian", "no"),
    ("Finnish", "fi"),
    ("Greek", "el"),
    ("Turkish", "tr"),
    ("Russian", "ru"),
    ("Ukrainian", "uk"),
    ("Arabic", "ar"),
    ("Hebrew", "he"),
    ("Hindi", "hi"),
    ("Thai", "th"),
    ("Vietnamese", "vi"),
    ("Indonesian", "id"),
    ("Japanese", "ja"),
    ("Korean", "ko"),
    ("Chinese (Simplified)", "zh-Hans"),
    ("Chinese (Traditional)", "zh-Hant"),
];

const REVERB: &[(&str, &str)] = &[("Parametric", "parametric"), ("Convolution", "convolution")];
const RAY_QUALITY: &[(&str, &str)] = &[("Low", "low"), ("Medium", "medium"), ("High", "high")];
const COMPRESSION: &[(&str, &str)] = &[("LZ4", "lz4"), ("Zstandard", "zstd"), ("None", "none")];
const AUTHORITY: &[(&str, &str)] = &[("Server", "server"), ("Shared", "shared")];
const ORIGIN: &[(&str, &str)] = &[("Off", "off"), ("Endless Worlds", "endless"), ("Every Level", "always")];
const TRAIL_QUALITY: &[(&str, &str)] = &[("Off", "off"), ("Low", "low"), ("Medium", "medium"), ("High", "high")];
const TRAIL_LOOK: &[(&str, &str)] = &[("Match the Water", "match_water"), ("Smooth", "smooth"), ("Blocky", "blocky")];
const WORLD_QUALITY: &[(&str, &str)] =
    &[("Low", "low"), ("Medium", "medium"), ("High", "high"), ("Ultra", "ultra"), ("Custom", "custom")];

const fn number(min: f64, max: f64, unit: &'static str) -> Kind {
    Kind::Number { min, max, unit, whole: false, slider: false }
}

const fn whole(min: f64, max: f64, unit: &'static str) -> Kind {
    Kind::Number { min, max, unit, whole: true, slider: false }
}

const fn slider(min: f64, max: f64, unit: &'static str) -> Kind {
    Kind::Number { min, max, unit, whole: false, slider: true }
}

/// Every setting but the modules', in the order the window shows them.
const FIXED: &[Setting] = &[
    // Audio
    Setting::new(
        ("Audio", "Sound"),
        "audio.headphones",
        "Headphones",
        Kind::Toggle,
        "3D sounds start out mixed for headphones, so a sound behind or above is heard there. Truer on headphones and odd on speakers; a game's options usually let the player choose (Audio.Headphones).",
    ),
    Setting::new(
        ("Audio", "Sound"),
        "audio.reverb",
        "Room reverb",
        Kind::Toggle,
        "3D sounds ring as the room the listener is in does: small rooms briefly, halls and caves for long. Costs a little CPU for each sound playing.",
    ),
    Setting::new(
        ("Audio", "Sound"),
        "audio.reverb_mode",
        "Reverb mode",
        Kind::Choice(REVERB),
        "Parametric rings each room from how long its echo lasts: cheap and smooth. Convolution plays the room's own echoes: truer in caves and halls, and heavier on the CPU.",
    ),
    Setting::new(
        ("Audio", "Ray-Traced Sound"),
        "audio.ray_traced",
        "Ray-traced sound",
        Kind::Toggle,
        "Sounds are blocked, bent round corners and echoed by the scene's real shapes, traced on the GPU. Needs a GPU with ray tracing, and costs up to the GPU budget each frame.",
    ),
    Setting::new(
        ("Audio", "Ray-Traced Sound"),
        "audio.ray_quality",
        "Quality",
        Kind::Choice(RAY_QUALITY),
        "How many rays each sound traces. Higher is steadier and truer, and fills the GPU budget sooner.",
    ),
    Setting::new(
        ("Audio", "Ray-Traced Sound"),
        "audio.ray_budget",
        "GPU budget",
        slider(0.0, 2.0, "ms"),
        "The most GPU time a frame tracing sound may take. More gives each sound more rays when many play; less leaves more of the frame for drawing.",
    ),
    Setting::new(
        ("Audio", "Builds"),
        "audio.steam_audio",
        "Steam Audio in builds",
        Kind::Toggle,
        "Builds take the Steam Audio library with them, for occlusion, sound paths and room reverb, where the project has the Steam Audio module. Off makes builds smaller, and those go quiet.",
    ),
    // Content
    Setting::new(
        ("Content", "Bundles"),
        "content.compression",
        "Compression",
        Kind::Choice(COMPRESSION),
        "How builds pack assets. LZ4 loads fastest. Zstandard makes smaller downloads and patches, and takes longer to build and a little longer to load. None is for quick development builds: any other build packs with LZ4.",
    ),
    Setting::new(
        ("Content", "Bundles"),
        "content.merge_below_kb",
        "Merge bundles under",
        whole(0.0, 65536.0, "KB"),
        "Bundles smaller than this merge into one that every one of their users opens anyway, saving files at no cost. 0 never merges.",
    ),
    Setting::new(
        ("Content", "Bundles"),
        "content.split_above_mb",
        "Split bundles over",
        whole(1.0, 65536.0, "MB"),
        "Bundles larger than this split, so patches and downloads stay small. Lower makes more, smaller files.",
    ),
    Setting::new(
        ("Content", "Roots"),
        "content.include",
        "Always include",
        Kind::Words,
        "Assets loaded by paths the build cannot see, which ship as roots of their own. Project paths where * matches within a folder, ** any number of folders and ? one letter.",
    )
    .advanced(),
    Setting::new(
        ("Content", "Roots"),
        "content.exclude",
        "Leave out",
        Kind::Words,
        "Assets that are not roots of their own, as patterns like Always include's. One that a shipped scene uses, or a behaviour names, still ships.",
    )
    .advanced(),
    // Foliage
    Setting::new(
        ("Foliage", "Wind"),
        "foliage.wind",
        "Wind",
        slider(0.0, 3.0, ""),
        "How hard the wind blows over plants on a fair day, in a level without a Wind Zone: 0.5 a light breeze, 1 one that sways trees. Rain and storms blow harder on top; 0 is still until the weather turns.",
    ),
    Setting::new(
        ("Foliage", "Wind"),
        "foliage.bend",
        "Bend",
        slider(0.0, 3.0, "×"),
        "How far plants bend in the wind and when pushed, times each plant's own Wind bend. 0 holds every plant still; trunks, rocks and solid blocks never bend.",
    ),
    Setting::new(
        ("Foliage", "Interaction"),
        "foliage.interaction",
        "Interaction",
        Kind::Toggle,
        "Characters, and Objs with a Foliage Interaction, push grass, bushes and leaves aside as they pass, and the plants spring back after them.",
    ),
    Setting::new(
        ("Foliage", "Interaction"),
        "foliage.push",
        "Push strength",
        slider(0.0, 3.0, "×"),
        "How hard what passes through pushes plants, times each one's own Strength.",
    )
    .advanced(),
    Setting::new(
        ("Foliage", "Interaction"),
        "foliage.spring",
        "Spring back",
        number(0.05, 10.0, "s"),
        "How long a pushed plant takes to spring most of the way back. Longer leaves a longer trail through a field.",
    )
    .advanced(),
    // Graphics
    Setting::new(
        ("Graphics", "Water Trails"),
        "water_trails.quality",
        "Quality",
        Kind::Choice(TRAIL_QUALITY),
        "How finely the ripples and foam things leave on water are worked out: Low, Medium and High keep 256, 512 and 1024 texels a side of the area round the camera, finer ripples for more GPU time and memory. Off leaves no trails.",
    ),
    Setting::new(
        ("Graphics", "Water Trails"),
        "water_trails.area",
        "Area",
        number(16.0, 512.0, "m"),
        "How far across the square of water round the camera trails cover. Larger keeps wakes further off, with coarser ripples at the same Quality.",
    ),
    Setting::new(
        ("Graphics", "Water Trails"),
        "water_trails.look",
        "Look",
        Kind::Choice(TRAIL_LOOK),
        "Blocky stands the water in small squares a quarter of a block across, stepping up and down with the ripples, its foam a square at a time. Match the Water is blocky on worlds of blocks and smooth on the sea, Pools and Streams.",
    ),
    Setting::new(
        ("Graphics", "Portals"),
        "portals.views",
        "Far sides at most",
        whole(0.0, 16.0, ""),
        "How many Portals' far sides a view draws at once, the nearest first; the rest show their mist. Each costs about as much as a view of its own.",
    ),
    Setting::new(
        ("Graphics", "Portals"),
        "portals.depth",
        "Depth",
        whole(1.0, 2.0, ""),
        "1 shows a Portal seen through a Portal as its mist; 2 draws its far side too, at the cost of more views.",
    ),
    Setting::new(
        ("Graphics", "Portals"),
        "portals.view_distance",
        "View distance",
        number(0.0, 100_000.0, "m"),
        "How far from its opening a camera sees a Portal's far side, for each Portal without a view distance of its own. Further shows more far sides at once, each costing a view.",
    ),
    Setting::new(
        ("Graphics", "Portals"),
        "portals.dimension_distance",
        "Dimension distance",
        number(2.0, 1000.0, "m"),
        "How near a camera comes to a Portal into another dimension, as a dungeon's gate, before that dimension loads. Only the nearest such Portal is open at a time, so one dimension at most is loaded besides the level's.",
    ),
    Setting::new(
        ("Graphics", "Portals"),
        "portals.draw_distance",
        "Draw distance",
        number(0.0, 100_000.0, "m"),
        "How far ahead a far side is drawn; past it shows the sky. 0 for as far as the world is. Shorter is cheaper.",
    ),
    Setting::new(
        ("Graphics", "Portals"),
        "portals.shadow_distance",
        "Shadow distance",
        number(0.0, 1000.0, "m"),
        "The sun casts shadows on a far side while the camera is within this of its opening; 0 for never. Each far side with shadows costs a shadow pass of its own.",
    )
    .advanced(),
    Setting::new(
        ("Graphics", "Portals"),
        "portals.far_side_distance",
        "Far side reach",
        number(32.0, 4096.0, "m"),
        "How far round a Portal's far side an endless world makes its own blocks. Further makes more blocks, costing memory and time.",
    )
    .advanced(),
    Setting::new(
        ("Graphics", "Portals"),
        "portals.water",
        "Water on far sides",
        Kind::Toggle,
        "Whether the water of a far side's block worlds is drawn. While any shows, it costs about as much again as the view's own water.",
    )
    .advanced(),
    // Input
    Setting::new(
        ("Input", "Actions"),
        "input.actions",
        "Actions file",
        Kind::Asset(AssetKind::InputActions),
        "The actions the game plays with. None uses the project's first .inputactions file by path, or Voxear's own where it has none.",
    ),
    Setting::new(
        ("Input", "Actions"),
        "input.open",
        "Actions and bindings",
        Kind::Button("Open Input Actions"),
        "Opens the Input Actions window, where actions, their bindings and their prompts are made.",
    ),
    // Localization
    Setting::new(
        ("Localization", "Languages"),
        "localization.languages",
        "Languages",
        Kind::Picks(LANGUAGES),
        "The languages the game is in, the one it is written in first. Its text comes from string tables; a game starts in the player's system language, or the nearest the project has.",
    ),
    Setting::new(
        ("Localization", "Fonts"),
        "localization.fonts",
        "Fonts",
        Kind::Part,
        "The fonts each language's text falls back to, in order, for letters its style's own font has not got: its own, then those of the languages it falls back to, then Voxear's faces, then the system's.",
    ),
    // Network
    Setting::new(
        ("Network", "Session"),
        "network.tick_rate",
        "Tick rate",
        whole(10.0, 240.0, "/s"),
        "How many times a second a networked game steps and sends what changed; fixed steps follow it while networked. Higher is smoother and quicker to answer, and costs CPU and bandwidth on every machine.",
    ),
    Setting::new(
        ("Network", "Session"),
        "network.port",
        "Port",
        whole(1.0, 65535.0, ""),
        "The UDP port a server or host listens on when none is given. Players connect to it, so a firewall must let it through.",
    ),
    Setting::new(
        ("Network", "Session"),
        "network.max_players",
        "Most players",
        whole(1.0, 1024.0, ""),
        "The most players a server takes at once; more are turned away.",
    ),
    Setting::new(
        ("Network", "Session"),
        "network.kilobytes_per_second",
        "Bandwidth",
        number(4.0, 100_000.0, "KB/s"),
        "The most each end sends the other a second. Higher lets more change each tick; lower suits slow connections, where less important changes wait.",
    ),
    Setting::new(
        ("Network", "Session"),
        "network.authority",
        "Authority",
        Kind::Choice(AUTHORITY),
        "Server decides everything, and players send what they press: hard to cheat, for games against others. Shared lets each player's machine run its own Objs: nothing waits on the server, for games played together.",
    ),
    // Physics
    Setting::new(
        ("Physics", "General"),
        "physics.gravity",
        "Gravity",
        Kind::Vector { count: 3, unit: "m/s²" },
        "The pull every rigidbody falls with as a world starts; scripts may change it with Physics.gravity. Earth's is 9.81 down. Taken up the next time the game plays.",
    ),
    Setting::new(
        ("Physics", "Layer Collision Matrix"),
        "physics.ignore",
        "Layer collision matrix",
        Kind::Part,
        "Which layers' Objs collide: untick a pair and their Objs pass through each other. Layers are named in Tags and Layers.",
    ),
    Setting::new(
        ("Physics 2D", "General"),
        "physics.gravity_2d",
        "Gravity",
        Kind::Vector { count: 2, unit: "m/s²" },
        "The pull every 2D rigidbody falls with as a world starts, for projects with the 2D Physics module. Taken up the next time the game plays.",
    ),
    // Player
    Setting::new(
        ("Player", "Identity"),
        "project.name",
        "Product name",
        Kind::Text,
        "The game's name: it titles its window and names its program and its save folder, unless a build profile names it otherwise. Networked games of different names never join.",
    ),
    Setting::new(
        ("Player", "Start"),
        "project.start_level",
        "Start level",
        Kind::Asset(AssetKind::Level),
        "The level the game opens on, unless a build profile picks another.",
    ),
    Setting::new(
        ("Player", "Builds"),
        "player.build",
        "Build profiles",
        Kind::Button("Open Build"),
        "Opens the Build window, where each build profile sets its platform, levels, window and version.",
    ),
    // Quality
    Setting::new(
        ("Quality", "Block Worlds"),
        "world.quality",
        "Quality",
        Kind::Choice(WORLD_QUALITY),
        "What making block worlds may spend: Low suits slow machines and laptops, Ultra fast desktops, and Custom keeps numbers set by hand. A game's options can change it as it runs.",
    ),
    Setting::new(
        ("Quality", "Block Worlds"),
        "world.machine",
        "This machine only",
        Kind::Toggle,
        "Keeps the numbers for this machine alone, beside the game's saves, in place of the project's, which are left as they are.",
    ),
    Setting::new(
        ("Quality", "Block Worlds"),
        "world.threads",
        "Threads",
        whole(1.0, 64.0, ""),
        "How many threads make terrain at once. More makes worlds sooner on many-core machines, and leaves fewer for the rest of the game.",
    )
    .advanced(),
    Setting::new(
        ("Quality", "Block Worlds"),
        "world.columns",
        "Columns at once",
        whole(1.0, 256.0, ""),
        "How many chunk columns an endless world makes at once. More fills the view sooner after a jump, using more memory meanwhile.",
    )
    .advanced(),
    Setting::new(
        ("Quality", "Block Worlds"),
        "world.upload",
        "Upload a frame",
        whole(1.0, 512.0, "MB"),
        "How many megabytes of chunks go to the GPU each frame; the rest come over the next frames, shown as they were until then. More catches up sooner, and risks a hitch.",
    )
    .advanced(),
    Setting::new(
        ("Quality", "Block Worlds"),
        "world.view_distance",
        "View distance",
        number(32.0, 4096.0, "m"),
        "The furthest an endless world's own chunks reach, at full detail. Further costs memory, making time and triangles.",
    )
    .advanced(),
    Setting::new(
        ("Quality", "Block Worlds"),
        "world.far_distance",
        "Far distance",
        number(32.0, 65_536.0, "m"),
        "The furthest its coarser rings reach: the far view. Further shows more of the land, costing memory and making time.",
    )
    .advanced(),
    Setting::new(
        ("Quality", "Block Worlds"),
        "world.near_detail",
        "Near detail",
        number(0.25, 4.0, "×"),
        "How many times as far as each world says its own chunks reach, within the view distance.",
    )
    .advanced(),
    Setting::new(
        ("Quality", "Block Worlds"),
        "world.far_detail",
        "Far detail",
        number(0.25, 4.0, "×"),
        "How many times as far as each world says its coarser rings reach, within the far distance.",
    )
    .advanced(),
    Setting::new(
        ("Quality", "Block Worlds"),
        "world.plant_detail",
        "Plant detail",
        number(0.25, 4.0, "×"),
        "How many times as far as each Foliage says its plants are drawn, the woods far off beginning further out too; and how many of the coarser rings structures are built into.",
    )
    .advanced(),
    Setting::new(
        ("Quality", "Block Worlds"),
        "world.far_block_pixels",
        "Far block size",
        number(0.0, 64.0, "px"),
        "The largest a coarser ring's blocks show where it begins, in pixels of a screen 1080 tall: the rings are pushed out, the world's own blocks drawn past them, until they are no larger. 0 draws them where the distances put them.",
    )
    .advanced(),
    Setting::new(
        ("Quality", "Particles"),
        "quality.particles",
        "Most particles",
        whole(0.0, voxear_render::MAX_PARTICLE_BUDGET as f64, ""),
        "The most simulated particles every Particle System may have between them; those there is no room for are not drawn. More costs GPU memory and time.",
    ),
    Setting::new(
        ("Quality", "Block Worlds"),
        "quality.block_lighting",
        "Lighting share",
        whole(voxear_render::MIN_LIGHTING_SHARE as f64, voxear_render::MAX_LIGHTING_SHARE as f64, "%"),
        "How much of the room every mesh shares a world of blocks' lighting may take. The rest is kept free for models spawned later, and the farthest chunks give theirs back as models take it.",
    )
    .advanced(),
    // Scripts
    Setting::new(
        ("Scripts", "C#"),
        "project.scripts",
        "C# project",
        Kind::Info,
        "The game's C# project, which the editor builds before Play and every build ships.",
    ),
    Setting::new(
        ("Scripts", "C#"),
        "scripts.open",
        "Code",
        Kind::Button("Open in Code Editor"),
        "Opens the project in the code editor Preferences ▸ External Tools picks.",
    ),
    // Tags and Layers
    Setting::new(
        ("Tags and Layers", "Layers"),
        "layers.names",
        "Layers",
        Kind::Part,
        "The project's 32 layers: name one to use it. An Obj's layer decides what it collides with (Physics ▸ Layer Collision Matrix) and what rays and cameras see.",
    ),
    // Time
    Setting::new(
        ("Time", "Steps"),
        "time.fixed_step",
        "Fixed step",
        number(1.0 / 480.0, 0.5, "s"),
        "How long each fixed step is: physics and FixedUpdate run this often, 0.02 being 50 a second. Shorter is smoother and more exact, and costs CPU; a networked game steps at its tick rate instead. Taken up the next time the game plays.",
    ),
    Setting::new(
        ("Time", "Steps"),
        "time.max_step",
        "Longest frame",
        number(0.01, 1.0, "s"),
        "A frame longer than this counts as this long, so a hitch or a breakpoint does not throw everything forward. Longer keeps the game's time to the clock on slow machines.",
    ),
    Setting::new(
        ("Time", "Steps"),
        "time.max_fixed_steps",
        "Fixed steps a frame",
        whole(1.0, 60.0, ""),
        "The most fixed steps one frame runs, the rest dropped, so a slow machine does not fall further and further behind. None are dropped while networked.",
    )
    .advanced(),
    // World
    Setting::new(
        ("World", "Floating Origin"),
        "floating_origin.mode",
        "Mode",
        Kind::Choice(ORIGIN),
        "When the world's middle moves with the camera: never, in levels with ground without end, or in every level. Taken up the next time the game plays.",
    ),
    Setting::new(
        ("World", "Floating Origin"),
        "floating_origin.distance",
        "Distance",
        number(1.0, 1_000_000.0, "m"),
        "How far the camera goes from the middle, along any axis, before the middle moves. Further moves it less often; past about 8 km, things that move begin to shake.",
    ),
    Setting::new(
        ("World", "Floating Origin"),
        "floating_origin.step",
        "Step",
        whole(1.0, 1_048_576.0, "m"),
        "How far the middle moves each time, in whole metres. The default is a whole number of terrain tiles and block chunks; others may show seams.",
    )
    .advanced(),
    Setting::new(
        ("Navigation", "Costs"),
        "navigation.road_cost",
        "Road",
        number(0.05, 1000.0, ""),
        "Ground marked as a road, by its block's Nav Area or a Nav Area box, a block of it at a time. Under 1 is sought out: at 0.6 a way along a road goes nearly twice as far to stay on it.",
    ),
    Setting::new(
        ("Navigation", "Costs"),
        "navigation.water_cost",
        "Water",
        number(0.05, 1000.0, ""),
        "Water a block deep, waded, a block of it at a time. Over 1 is kept off where there is another way; plain ground costs 1.",
    ),
    Setting::new(
        ("Navigation", "Costs"),
        "navigation.swim_cost",
        "Swim",
        number(0.05, 1000.0, ""),
        "Water too deep to stand in, swum by what swims, a block of it at a time. Over 1 is kept off where there is another way.",
    ),
    Setting::new(
        ("Navigation", "Costs"),
        "navigation.door_cost",
        "Door",
        number(0.05, 1000.0, ""),
        "A shut door, opened to go through by what opens doors. Over 1 goes round by another way where it is not much longer.",
    ),
    Setting::new(
        ("Navigation", "Your Own Areas"),
        "navigation.custom_1_name",
        "Area 1 name",
        Kind::Text,
        "What the project calls its own area 1, as blocks' Nav Area and Nav Area boxes offer it as Custom 1.",
    ),
    Setting::new(
        ("Navigation", "Your Own Areas"),
        "navigation.custom_1_cost",
        "Area 1 cost",
        number(0.05, 1000.0, ""),
        "What a block of area 1 costs to cross: plain ground costs 1, under 1 is sought out, over 1 kept off.",
    ),
    Setting::new(
        ("Navigation", "Your Own Areas"),
        "navigation.custom_2_name",
        "Area 2 name",
        Kind::Text,
        "What the project calls its own area 2, as blocks' Nav Area and Nav Area boxes offer it as Custom 2.",
    ),
    Setting::new(
        ("Navigation", "Your Own Areas"),
        "navigation.custom_2_cost",
        "Area 2 cost",
        number(0.05, 1000.0, ""),
        "What a block of area 2 costs to cross: plain ground costs 1, under 1 is sought out, over 1 kept off.",
    ),
    Setting::new(
        ("Navigation", "Your Own Areas"),
        "navigation.custom_3_name",
        "Area 3 name",
        Kind::Text,
        "What the project calls its own area 3, as blocks' Nav Area and Nav Area boxes offer it as Custom 3.",
    ),
    Setting::new(
        ("Navigation", "Your Own Areas"),
        "navigation.custom_3_cost",
        "Area 3 cost",
        number(0.05, 1000.0, ""),
        "What a block of area 3 costs to cross: plain ground costs 1, under 1 is sought out, over 1 kept off.",
    ),
    Setting::new(
        ("Navigation", "Your Own Areas"),
        "navigation.custom_4_name",
        "Area 4 name",
        Kind::Text,
        "What the project calls its own area 4, as blocks' Nav Area and Nav Area boxes offer it as Custom 4.",
    ),
    Setting::new(
        ("Navigation", "Your Own Areas"),
        "navigation.custom_4_cost",
        "Area 4 cost",
        number(0.05, 1000.0, ""),
        "What a block of area 4 costs to cross: plain ground costs 1, under 1 is sought out, over 1 kept off.",
    ),
    Setting::new(
        ("Navigation", "Budgets"),
        "navigation.searches_a_step",
        "Searches a step",
        whole(1.0, 100_000.0, ""),
        "How many ways agents may begin to look for each step, on navigation's threads: a crowd sent off at once has its ways found over the next few steps. More finds a crowd's ways sooner, and costs more threads' time in a step.",
    ),
    Setting::new(
        ("Navigation", "Budgets"),
        "navigation.searches_here",
        "Done in the step",
        whole(0.0, 100_000.0, ""),
        "How many searches no thread has begun by the step after they were asked for are done in the step itself, so an agent sets off then. More costs the step itself more where many ask at once.",
    ),
    Setting::new(
        ("Navigation", "Budgets"),
        "navigation.most_looked_at",
        "Places a search looks at",
        whole(100.0, 10_000_000.0, ""),
        "How many places one search may look at before it gives up and takes the nearest it reached: a way through a great maze may need more. More lets a search cost more.",
    )
    .advanced(),
];

static SETTINGS: LazyLock<Vec<Setting>> = LazyLock::new(|| {
    let modules = Module::ALL.into_iter().map(|module| {
        let key: &'static str = Box::leak(format!("modules.{}", module.key()).into_boxed_str());
        let tip: &'static str = Box::leak(
            format!(
                "{}. Off leaves its components out of Add Component and its assemblies out of the scripts, which build again.",
                module.summary()
            )
            .into_boxed_str(),
        );
        Setting::new(("Modules", "Modules"), key, module.title(), Kind::Toggle, tip)
    });
    // The agent types after the rest of Navigation: the Default one's sizes, and each of the project's own.
    let leak = |text: String| -> &'static str { Box::leak(text.into_boxed_str()) };
    let sizes = |prefix: &str, section: &'static str, whose: &str| {
        [
            ("height", "Height", number(0.1, 100.0, "m"), "How tall it is: what room it needs over a floor."),
            (
                "radius",
                "Radius",
                number(0.05, 50.0, "m"),
                "How wide it is round its middle: how far its ways keep from walls and edges.",
            ),
            (
                "step",
                "Step",
                number(0.0, 10.0, "m"),
                "How high a step it walks up, so stairs join one floor to the next.",
            ),
            ("slope", "Slope", number(0.0, 89.0, "°"), "The steepest slope it walks up, in degrees."),
        ]
        .map(|(key, label, kind, tip)| {
            let tip = leak(format!("{tip} What {whose} level's nav mesh is baked for; bake again to take it up."));
            Setting::new(("Navigation", section), leak(format!("navigation.{prefix}{key}")), label, kind, tip)
        })
    };
    let mut types: Vec<Setting> = sizes("agent_", "Default Agent", "the Default agent type's").to_vec();
    for slot in 1..voxear_engine::navigation::AGENT_TYPES {
        let section = leak(format!("Agent Type {slot}"));
        let prefix = format!("agent_{slot}_");
        let named = "What the project calls it, as a Nav Agent's Agent Type offers it. Unnamed, it is not baked for.";
        types.push(Setting::new(
            ("Navigation", section),
            leak(format!("navigation.{prefix}name")),
            "Name",
            Kind::Text,
            named,
        ));
        types.extend(sizes(&prefix, section, "its"));
        for (area, label) in [("road", "Road"), ("water", "Water"), ("swim", "Swim")] {
            let tip = leak(format!(
                "What a block of {} costs it to cross, in place of the project's: under 1 sought out, over 1 kept off. 0 for the project's.",
                label.to_lowercase()
            ));
            let key = leak(format!("navigation.{prefix}{area}_cost"));
            types.push(Setting::new(
                ("Navigation", section),
                key,
                leak(format!("{label} cost")),
                number(0.0, 1000.0, ""),
                tip,
            ));
        }
    }
    let after = FIXED.iter().rposition(|setting| setting.category == "Navigation").map_or(FIXED.len(), |at| at + 1);
    let fixed: Vec<Setting> =
        FIXED[..after].iter().copied().chain(types).chain(FIXED[after..].iter().copied()).collect();
    let at = fixed.iter().position(|setting| setting.category == "Network").unwrap_or(fixed.len());
    fixed[..at].iter().copied().chain(modules).chain(fixed[at..].iter().copied()).collect()
});

/// Every setting Project Settings shows, in order.
pub fn settings() -> &'static [Setting] {
    &SETTINGS
}

/// The setting whose key this is.
pub fn find(key: &str) -> Option<&'static Setting> {
    settings().iter().find(|setting| setting.key == key)
}

/// Each setting's value now and its default, by key, as the window and
/// `vox editor call` show them.
pub type Values = HashMap<&'static str, (Json, Json)>;

/// Every setting's value in `game`'s project now, and its default.
pub fn values(game: &Game) -> Values {
    let file = toml::Value::try_from(&game.project.file).unwrap_or(toml::Value::Table(Default::default()));
    let defaults = toml::Value::try_from(voxear_engine::ProjectFile::defaults(""))
        .unwrap_or(toml::Value::Table(Default::default()));
    let db = game.content.db();
    let world = &game.engine.world;
    let (budget, machine) = (WorldBudget::of(world), MachineBudget::of(world).kept);
    let budget_json = serde_json::to_value(budget).unwrap_or_default();
    let default_budget = serde_json::to_value(WorldBudget::default()).unwrap_or_default();
    let modules = game.project.modules();
    let mut values = Values::new();
    for setting in settings() {
        let (table, name) = setting.key.split_once('.').unwrap_or((setting.key, ""));
        let pair = match (table, name) {
            // A module is the project's choice either way, never marked as changed.
            ("modules", key) => {
                let has = Module::from_key(key).is_some_and(|module| modules.has(module));
                (json!(has), json!(has))
            }
            ("project", "name") => (json!(game.project.name()), json!(game.project.name())),
            ("world", "machine") => (json!(machine), json!(false)),
            ("world", "quality") => {
                let label = |quality: WorldQuality| json!(quality.label());
                (label(budget.quality), label(WorldQuality::default()))
            }
            ("world", field) => (rounded(&budget_json[field]), rounded(&default_budget[field])),
            ("project", "scripts") => {
                let shown = game.project.file.project.scripts.as_ref().map(|path| path.display().to_string());
                (json!(shown.unwrap_or_else(|| "None".into())), Json::Null)
            }
            _ if !setting.settable() => (Json::Null, Json::Null),
            _ => {
                let read =
                    |file: &toml::Value| file.get(table).and_then(|t| t.get(name)).map(|v| to_json(setting, v, db));
                (read(&file).unwrap_or(Json::Null), read(&defaults).unwrap_or(Json::Null))
            }
        };
        values.insert(setting.key, pair);
    }
    values
}

/// A number read from an `f32` as it was written: 0.02, not
/// 0.019999999552965164.
fn rounded(value: &Json) -> Json {
    match value.as_f64() {
        Some(n) if value.is_f64() => format!("{}", n as f32).parse::<f64>().map_or(value.clone(), |n| json!(n)),
        _ => value.clone(),
    }
}

/// A value as `voxear.toml` writes it, as the window shows it: a choice by
/// its label, an asset by its path, numbers as written.
fn to_json(setting: &Setting, value: &toml::Value, db: &AssetDb) -> Json {
    use toml::Value;
    match (setting.kind, value) {
        (Kind::Choice(options), Value::String(word)) => json!(shown_label(options, word).unwrap_or(word)),
        (Kind::Picks(options), Value::Array(items)) => {
            let shown = items.iter().filter_map(Value::as_str).map(|word| shown_label(options, word).unwrap_or(word));
            json!(shown.collect::<Vec<_>>())
        }
        (Kind::Asset(_), Value::String(reference)) => {
            let path = reference.parse().ok().and_then(|guid| db.get(guid)).map(|entry| entry.path.clone());
            json!(path.unwrap_or_else(|| reference.clone()))
        }
        (_, Value::Float(n)) => rounded(&json!(n)),
        (_, Value::Array(items)) => Json::Array(items.iter().map(|item| to_json(setting, item, db)).collect()),
        _ => serde_json::to_value(value).unwrap_or_default(),
    }
}

/// A checked value (see `setting_info::check`) as `voxear.toml` writes it,
/// the same kind of value as the setting's default; None for none, which
/// takes it out of the file.
pub fn to_toml(setting: &Setting, value: &Json, db: &AssetDb) -> Result<Option<toml::Value>, String> {
    use toml::Value;
    let default = Project::default_setting(setting.key);
    let whole = matches!(default, Some(Value::Integer(_)));
    let text = |value: &Json| value.as_str().unwrap_or_default().to_string();
    Ok(Some(match setting.kind {
        _ if value.is_null() => return Ok(None),
        Kind::Toggle => Value::Boolean(value.as_bool().unwrap_or_default()),
        Kind::Choice(options) => Value::String(kept_word(options, &text(value)).unwrap_or_default().to_string()),
        Kind::Number { .. } if whole => Value::Integer(value.as_f64().unwrap_or_default().round() as i64),
        Kind::Number { .. } => Value::Float(value.as_f64().unwrap_or_default()),
        Kind::Vector { .. } => {
            let numbers = value.as_array().into_iter().flatten().map(|n| Value::Float(n.as_f64().unwrap_or_default()));
            Value::Array(numbers.collect())
        }
        Kind::Text => Value::String(text(value)),
        Kind::Asset(_) => {
            let path = text(value);
            let guid = db.guid_of(&path).ok_or_else(|| format!("no asset is at {path}"))?;
            Value::String(guid.to_string())
        }
        Kind::Words => {
            Value::Array(value.as_array().into_iter().flatten().map(|word| Value::String(text(word))).collect())
        }
        Kind::Picks(options) => {
            let words = value.as_array().into_iter().flatten();
            let kept = words.map(|word| Value::String(kept_word(options, &text(word)).unwrap_or_default().to_string()));
            Value::Array(kept.collect())
        }
        Kind::Color | Kind::Folder | Kind::Info | Kind::Part | Kind::Button(_) => {
            return Err(format!("{} is not kept in voxear.toml", setting.label));
        }
    }))
}

/// The block worlds' budget with one of its numbers, by its key, changed:
/// Custom from then on, within what each may be.
pub fn budget_with(budget: WorldBudget, field: &str, value: f64) -> Option<WorldBudget> {
    let mut json = serde_json::to_value(budget).ok()?;
    json.get(field)?;
    json[field] =
        if matches!(field, "threads" | "columns" | "upload") { json!(value.round() as u64) } else { json!(value) };
    json["quality"] = json!("custom");
    serde_json::from_value::<WorldBudget>(json).ok().map(WorldBudget::checked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_setting_has_a_category_listed_a_tooltip_and_a_unique_key() {
        let mut keys = std::collections::HashSet::new();
        for setting in settings() {
            assert!(CATEGORIES.iter().any(|c| c.name == setting.category), "{} has no category", setting.key);
            assert!(setting.tip.len() > 20 && setting.tip.ends_with('.'), "{} has a tooltip", setting.key);
            assert!(keys.insert(setting.key), "{} once", setting.key);
        }
        for category in CATEGORIES {
            assert!(settings().iter().any(|s| s.category == category.name), "{} has settings", category.name);
        }
        assert!(find("modules.water").is_some_and(|water| water.label == "Water"), "a toggle for each module");
        // Every setting kept as a plain key of the file has a default there.
        for setting in settings().iter().filter(|s| s.settable()) {
            let special = ["modules.", "world.", "project.name", "project.start_level", "input.actions"];
            if special.iter().any(|start| setting.key.starts_with(start)) {
                continue;
            }
            assert!(Project::default_setting(setting.key).is_some(), "{} has a default in voxear.toml", setting.key);
        }
    }

    #[test]
    fn a_budget_number_changed_makes_it_custom_and_keeps_it_in_bounds() {
        let low = WorldQuality::Low.budget().unwrap();
        let more = budget_with(low, "threads", 3.4).unwrap();
        assert_eq!((more.quality, more.threads, more.columns), (WorldQuality::Custom, 3, low.columns));
        assert_eq!(budget_with(low, "near_detail", 99.0).unwrap().near_detail, 4.0, "within bounds");
        assert!(budget_with(low, "nonsense", 1.0).is_none());
    }
}
