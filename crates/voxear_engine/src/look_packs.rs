//! Look packs: how a game looks, chosen by whoever plays it. A `.lookpack`
//! is a few of a level's lighting settings, laid over whatever lighting the
//! level has: a warmer sun, tinted shadows, clouds built of cubes, more
//! bloom, faces of small cubes. The level keeps its time of day, its
//! weather and everything the pack leaves out, so one pack suits every
//! level of a game, and a player's choice goes with them from level to
//! level.
//!
//! ```toml
//! format = 1
//! description = "Warm light, soft blue shade and clouds built of cubes."
//!
//! [sun]
//! tint = [1.0, 0.96, 0.88]
//! shadow_tint = [0.05, 0.08, 0.16]
//!
//! [sky.clouds]
//! kind = "blocks"
//!
//! [post.bloom]
//! intensity = 0.14
//!
//! [blocks]
//! voxels = 8
//! ```
//!
//! `[sky]`, `[sun]` and `[post]` are written as a `.lighting` file writes
//! them (see `lighting`), and only what is written changes. `[blocks]` sets
//! how every Block World's faces are cut into small cubes. A pack that
//! names a `look` starts from that built-in look in place of the level's
//! own lighting, for a pack that makes every level look one way.
//!
//! A pack comes from one of three places: the engine's own, built in
//! (`builtin:sunlit`); the game's, a `.lookpack` asset in its project; or
//! the player's, a `.lookpack` file they put in the game's `Looks` folder,
//! beside its saves (`file:` and its path). [`list`] gives them all, for a
//! menu.
//!
//! The pack worn is the world's [`Wearing`]. Each tick [`update`] lays it
//! over the level's sky, sun look and post-processing as they are then, and
//! leaves the result in the world as the renderer's `WornLook`; the level's
//! own stay as they are, for the editor and scripts.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use voxear_assets::AssetKind;
use voxear_ecs::World;
use voxear_render::{BlockVoxels, Look, LookName, PostProcess, Sky, SunLook, WornLook};

use crate::{
    content::Content,
    lighting::{SunData, merge, plain_sun},
    scene_file::{SkyData, sky_data, sky_from_data},
};

/// The `.lookpack` format this version writes and reads.
pub const LOOK_PACK_FORMAT: u32 = 1;

/// What a look pack's file name ends with.
pub const LOOK_PACK_EXTENSION: &str = "lookpack";

/// The folder a player puts look packs of their own in, beside the game's
/// saves.
pub const LOOKS_FOLDER: &str = "Looks";

/// What a pack read from a file outside the game is named by, before its
/// path.
pub const FILE_PREFIX: &str = "file:";

/// The engine's own packs: what each is named by after `builtin:`, what a
/// menu calls it, and its file.
pub const BUILTIN: [(&str, &str, &str); 6] = [
    ("sunlit", "Sunlit", include_str!("look_packs/sunlit.lookpack")),
    ("cube_clouds", "Cube Clouds", include_str!("look_packs/cube_clouds.lookpack")),
    ("little_cubes", "Little Cubes", include_str!("look_packs/little_cubes.lookpack")),
    ("painted", "Painted", include_str!("look_packs/painted.lookpack")),
    ("old_film", "Old Film", include_str!("look_packs/old_film.lookpack")),
    ("plain", "Plain", include_str!("look_packs/plain.lookpack")),
];

/// A `.lookpack` file as written.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookPackFile {
    #[serde(default = "look_pack_format")]
    pub format: u32,
    /// A line for the menu a player picks it from.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// A built-in look the pack starts from in place of the level's own
    /// lighting, such as `"storybook"`; none for a pack laid over it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look: Option<String>,
    /// The sky's settings, as a scene's `[sky]` writes them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sky: Option<toml::Table>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sun: Option<SunData>,
    /// Post-processing, as a scene's `[post]` writes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post: Option<toml::Table>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocks: Option<BlocksLook>,
}

fn look_pack_format() -> u32 {
    LOOK_PACK_FORMAT
}

/// How a pack has every Block World's faces drawn.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlocksLook {
    /// How many small cubes a block's side is drawn as; 0 for plain faces.
    #[serde(default = "usual_voxels")]
    pub voxels: u32,
    /// How many small cubes deep the deepest sink.
    #[serde(default = "usual_relief")]
    pub relief: f32,
}

/// What a `[blocks]` that names no number of small cubes gives a block's
/// side: a pack that writes the section wants them.
fn usual_voxels() -> u32 {
    8
}

fn usual_relief() -> f32 {
    BlockVoxels::default().relief
}

impl LookPackFile {
    /// Reads a pack's file, and checks it can be worn: laid over the plain
    /// defaults, every setting it writes must be one there is.
    pub fn parse(text: &str) -> Result<Self> {
        let file: Self = toml::from_str(text)?;
        ensure!(file.format <= LOOK_PACK_FORMAT, "written by a newer Voxear (format {})", file.format);
        file.worn(Sky::default(), None, PostProcess::default())?;
        Ok(file)
    }

    /// One of the engine's own packs, by its name.
    pub fn builtin(name: &str) -> Result<Self> {
        let Some((.., text)) = BUILTIN.iter().find(|(key, ..)| *key == name) else {
            bail!("no built-in look pack named `{name}`");
        };
        Self::parse(text).with_context(|| format!("reading the built-in look pack {name}"))
    }

    /// A level's `sky`, `sun` look and `post`-processing as they are with
    /// the pack over them: what the pack writes, and the level's own in
    /// everything else.
    pub fn worn(&self, sky: Sky, sun: Option<SunLook>, post: PostProcess) -> Result<WornLook> {
        let look = self.look.as_deref().map(LookName::parse).transpose()?.map(Look::of);
        let lut = post.grading.lut;
        let (mut sky, mut sun, mut post) = match look {
            Some(look) => (look.sky, Some(look.sun), look.post),
            None => (sky, sun, post),
        };
        if let Some(over) = &self.sky {
            let mut table = toml::Table::try_from(sky_data(&sky)).context("writing the sky")?;
            merge(&mut table, over);
            let data: SkyData = table.try_into().context("reading the pack's [sky]")?;
            sky = sky_from_data(&data);
        }
        if let Some(over) = self.sun {
            let mut looked = sun.unwrap_or_else(plain_sun);
            over.lay_over(&mut looked);
            sun = Some(looked);
        }
        if let Some(over) = &self.post {
            let named = ["look", "preset"].into_iter().find(|key| over.contains_key(*key));
            ensure!(
                named.is_none(),
                "[post] names a {}; a pack's own `look` goes at its top",
                named.unwrap_or_default()
            );
            let mut table = toml::Table::try_from(post).context("writing the post-processing")?;
            merge(&mut table, over);
            post = toml::Value::Table(table).try_into().context("reading the pack's [post]")?;
        }
        // A lookup table is an asset the level loaded: a pack keeps it.
        post.grading.lut = lut;
        let voxels = self.blocks.map(|blocks| BlockVoxels {
            count: blocks.voxels.min(BlockVoxels::MOST),
            relief: blocks.relief.clamp(0.0, BlockVoxels::DEEPEST),
        });
        Ok(WornLook { sky, sun, post, voxels })
    }
}

/// The level's lighting a worn look was last worked out from.
type Seen = (SkyData, Option<SunLook>, PostProcess);

/// The look pack the player wears, as a world resource: what names it, and
/// its file. None in the world for none worn.
#[derive(Clone, Debug)]
pub struct Wearing {
    pub reference: String,
    pub pack: LookPackFile,
    seen: Option<Seen>,
}

impl Wearing {
    pub fn new(reference: &str, pack: LookPackFile) -> Self {
        Self { reference: reference.to_string(), pack, seen: None }
    }
}

/// Lays the pack worn over the level's lighting as it is now, for the
/// renderer to draw with: again only when the level's lighting or the pack
/// has changed. With none worn, the level is drawn as it is.
pub fn update(world: &mut World) {
    let Some(wearing) = world.resource::<Wearing>() else {
        if world.resource::<WornLook>().is_some() {
            world.remove_resource::<WornLook>();
        }
        return;
    };
    let sky = world.resource::<Sky>().copied().unwrap_or_default();
    let sun = world.resource::<SunLook>().copied();
    let post = world.resource::<PostProcess>().copied().unwrap_or_default();
    let now: Seen = (sky_data(&sky), sun, post);
    if wearing.seen.as_ref() == Some(&now) {
        return;
    }
    let worn = wearing.pack.worn(sky, sun, post);
    let reference = wearing.reference.clone();
    if let Some(wearing) = world.resource_mut::<Wearing>() {
        wearing.seen = Some(now);
    }
    match worn {
        Ok(worn) => drop(world.insert_resource(worn)),
        Err(error) => {
            tracing::error!("the look pack {reference} cannot be worn over this level: {error:#}");
            world.remove_resource::<WornLook>();
        }
    }
}

/// The pack `reference` names: `builtin:` and a name, `file:` and the path
/// of a file outside the game, or a `.lookpack` asset.
pub fn load(content: &mut Content, reference: &str) -> Result<LookPackFile> {
    if let Some(name) = reference.strip_prefix("builtin:") {
        return LookPackFile::builtin(name);
    }
    if let Some(path) = reference.strip_prefix(FILE_PREFIX) {
        let text = std::fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
        return LookPackFile::parse(&text).with_context(|| format!("reading {path}"));
    }
    let guid = content.parse(reference)?.guid().with_context(|| format!("`{reference}` is not a look pack"))?;
    // A server build leaves look packs out: nothing is drawn.
    if content.is_stripped(guid) {
        return Ok(LookPackFile::default());
    }
    let (text, path) = content.asset_text(guid, AssetKind::LookPack)?;
    LookPackFile::parse(&text).with_context(|| format!("reading {path}"))
}

/// Wears the pack `reference` names from the next frame, or with an empty
/// reference none. A pack that cannot be read leaves the one worn as it is.
pub fn wear(world: &mut World, content: &mut Content, reference: &str) -> Result<()> {
    if reference.is_empty() {
        world.remove_resource::<Wearing>();
        return Ok(());
    }
    let pack = load(content, reference).with_context(|| format!("loading the look pack {reference}"))?;
    world.insert_resource(Wearing::new(&lasting(content, reference), pack));
    Ok(())
}

/// What names a pack for as long as it is there: an asset by its GUID,
/// which a new name or place for its file leaves as it is, and any other as
/// it is named.
fn lasting(content: &Content, reference: &str) -> String {
    if reference.starts_with("builtin:") || reference.starts_with(FILE_PREFIX) {
        return reference.to_string();
    }
    let guid = content.parse(reference).ok().and_then(|asset| asset.guid());
    guid.map_or_else(|| reference.to_string(), |guid| guid.to_string())
}

/// What names the pack worn; none for none.
pub fn worn(world: &World) -> Option<&str> {
    world.resource::<Wearing>().map(|wearing| wearing.reference.as_str())
}

/// The file beside the saves that keeps the look pack the player chose.
const LOOK_FILE: &str = "look.txt";

/// What names the look pack the player last chose, kept beside the saves
/// so a game starts in it again; none when they chose none.
pub fn remembered() -> Option<String> {
    let reference = std::fs::read_to_string(voxear_scripting::saves_folder()?.join(LOOK_FILE)).ok()?;
    Some(reference.trim().to_string()).filter(|reference| !reference.is_empty())
}

/// Keeps the look pack the player chose, for [`remembered`]; an empty
/// reference for none.
pub fn remember(reference: &str) {
    let Some(folder) = voxear_scripting::saves_folder() else { return };
    let kept = std::fs::create_dir_all(&folder).and_then(|()| std::fs::write(folder.join(LOOK_FILE), reference));
    if let Err(error) = kept {
        tracing::warn!("could not keep the look pack chosen: {error}");
    }
}

/// Where a pack is from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PackSource {
    /// The engine's own.
    Builtin,
    /// A `.lookpack` asset of the game's.
    Game,
    /// A file in the player's `Looks` folder.
    Player,
}

/// A pack as a menu lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PackInfo {
    /// What [`wear`] takes to wear it.
    pub reference: String,
    pub name: String,
    pub description: String,
    pub source: PackSource,
}

/// Where a player's own packs are kept for a game whose saves are in
/// `saves`: `Looks`, beside them.
pub fn player_folder(saves: &Path) -> Option<PathBuf> {
    Some(saves.parent()?.join(LOOKS_FOLDER))
}

/// Every pack there is to wear: the engine's, then the game's by name,
/// then those in the player's `folder` by name. A player's file that cannot
/// be read is left out, with a warning.
pub fn list(content: &mut Content, folder: Option<&Path>) -> Vec<PackInfo> {
    let mut packs: Vec<PackInfo> = BUILTIN
        .iter()
        .map(|(key, title, text)| PackInfo {
            reference: format!("builtin:{key}"),
            name: title.to_string(),
            description: LookPackFile::parse(text).map(|file| file.description).unwrap_or_default(),
            source: PackSource::Builtin,
        })
        .collect();
    let named = |path: &Path| path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
    let assets: Vec<_> = content
        .db()
        .entries()
        .filter(|entry| entry.kind == AssetKind::LookPack)
        .map(|entry| (entry.guid, named(Path::new(&entry.path))))
        .collect();
    let mut game = Vec::new();
    for (guid, name) in assets {
        if content.is_stripped(guid) {
            continue;
        }
        let read = content.asset_text(guid, AssetKind::LookPack).ok();
        let file = read.and_then(|(text, _)| toml::from_str::<LookPackFile>(&text).ok());
        let description = file.map(|file| file.description).unwrap_or_default();
        game.push(PackInfo { reference: guid.to_string(), name, description, source: PackSource::Game });
    }
    game.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    packs.extend(game);
    packs.extend(folder.map(player_packs).unwrap_or_default());
    packs
}

/// Every pack there is to wear, which one the world wears and where a
/// player keeps their own, as JSON for scripts: `packs`, `worn` and
/// `folder`.
pub fn json(content: &mut Content, world: &World, folder: Option<&Path>) -> String {
    serde_json::json!({
        "packs": list(content, folder),
        "worn": worn(world),
        "folder": folder.map(|folder| folder.display().to_string()),
    })
    .to_string()
}

/// The packs in a player's `folder`, by name.
fn player_packs(folder: &Path) -> Vec<PackInfo> {
    let Ok(files) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut packs = Vec::new();
    for path in files.flatten().map(|file| file.path()) {
        if path.extension().and_then(|e| e.to_str()) != Some(LOOK_PACK_EXTENSION) {
            continue;
        }
        let read = std::fs::read_to_string(&path).map_err(anyhow::Error::from);
        match read.and_then(|text| LookPackFile::parse(&text)) {
            Ok(file) => packs.push(PackInfo {
                reference: format!("{FILE_PREFIX}{}", path.display()),
                name: path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default(),
                description: file.description,
                source: PackSource::Player,
            }),
            Err(error) => tracing::warn!("the look pack {} is left out: {error:#}", path.display()),
        }
    }
    packs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    packs
}

#[cfg(test)]
mod tests {
    use voxear_render::CloudKind;

    use super::*;

    fn pack(text: &str) -> LookPackFile {
        LookPackFile::parse(text).unwrap()
    }

    #[test]
    fn a_pack_changes_what_it_writes_and_leaves_the_levels_lighting_in_all_else() {
        let golden = Look::of(LookName::GoldenHour);
        let text = "[sky.clouds]\nkind = \"blocks\"\n[sun]\nstrength = 2.0\n[post.bloom]\nintensity = 0.5\n";
        let worn = pack(text).worn(golden.sky, Some(golden.sun), golden.post).unwrap();
        // The pack's own.
        assert_eq!(worn.sky.clouds.kind, CloudKind::Blocks);
        assert_eq!(worn.sun.map(|sun| sun.strength), Some(2.0));
        assert_eq!(worn.post.bloom.intensity, 0.5);
        // The level's, in everything it leaves out.
        assert_eq!(
            (worn.sky.clouds.cover, worn.sky.clouds.height),
            (golden.sky.clouds.cover, golden.sky.clouds.height)
        );
        assert!((worn.sky.haze - golden.sky.haze).abs() < 1e-4 && worn.sky.mode == golden.sky.mode);
        let sun = worn.sun.unwrap();
        assert_eq!(
            (sun.tint, sun.softness, sun.shadow_distance),
            (golden.sun.tint, golden.sun.softness, golden.sun.shadow_distance)
        );
        assert_eq!(worn.post.bloom.scatter, golden.post.bloom.scatter);
        assert_eq!(
            (worn.post.fog, worn.post.tonemap, worn.post.exposure),
            (golden.post.fog, golden.post.tonemap, golden.post.exposure)
        );
        assert_eq!(worn.voxels, None, "each world's own faces");
    }

    #[test]
    fn an_empty_pack_changes_nothing_and_a_level_with_no_sun_look_gets_one_only_from_a_pack_that_writes_it() {
        let night = Look::of(LookName::NightRaid);
        let worn = pack("format = 1").worn(night.sky, None, night.post).unwrap();
        assert_eq!((worn.post, worn.sun), (night.post, None));
        assert_eq!(sky_data(&worn.sky), sky_data(&night.sky));
        let warm = pack("[sun]\nshadow_tint = [0.1, 0.1, 0.3]").worn(night.sky, None, night.post).unwrap();
        let sun = warm.sun.expect("the pack's");
        assert!(sun.shadow_tint.z > sun.shadow_tint.x && sun.strength == 1.0, "over a plain sun: {sun:?}");
    }

    #[test]
    fn a_pack_cuts_every_worlds_faces_into_small_cubes_within_what_can_be_drawn() {
        let worn = |text: &str| pack(text).worn(Sky::default(), None, PostProcess::default()).unwrap().voxels;
        assert_eq!(worn("[blocks]\nvoxels = 8"), Some(BlockVoxels { count: 8, relief: BlockVoxels::default().relief }));
        assert_eq!(worn("[blocks]\nvoxels = 16\nrelief = 0.5"), Some(BlockVoxels { count: 16, relief: 0.5 }));
        assert_eq!(
            worn("[blocks]\nvoxels = 0"),
            Some(BlockVoxels { count: 0, relief: 2.0 }),
            "plain faces, whatever the world's"
        );
        let wild = worn("[blocks]\nvoxels = 4000\nrelief = 99.0").unwrap();
        assert_eq!((wild.count, wild.relief), (BlockVoxels::MOST, BlockVoxels::DEEPEST));
    }

    #[test]
    fn a_pack_that_names_a_look_starts_from_it_in_place_of_the_levels_lighting() {
        let night = Look::of(LookName::NightRaid);
        let story = Look::of(LookName::Storybook);
        let worn =
            pack("look = \"storybook\"\n[post.vignette]\nintensity = 0.9").worn(night.sky, Some(night.sun), night.post);
        let worn = worn.unwrap();
        assert_eq!(worn.sun, Some(story.sun));
        assert_eq!((worn.post.tonemap, worn.post.vignette.intensity), (story.post.tonemap, 0.9));
        assert_eq!(sky_data(&worn.sky), sky_data(&story.sky));
    }

    #[test]
    fn a_pack_that_writes_what_there_is_not_is_not_read() {
        assert!(LookPackFile::parse("[post.bloom]\nglare = 2.0").is_err(), "no such setting");
        assert!(LookPackFile::parse("[sky.clouds]\nkind = \"pyramids\"").is_err(), "no such kind of cloud");
        assert!(LookPackFile::parse("look = \"gloomy\"").is_err(), "no such look");
        assert!(LookPackFile::parse("[post]\nlook = \"storybook\"").is_err(), "a look goes at the top");
        assert!(LookPackFile::parse("[weather]\nkind = \"rain\"").is_err(), "the weather is the level's");
        assert!(LookPackFile::parse("format = 99").is_err(), "a newer Voxear's");
    }

    #[test]
    fn every_built_in_pack_reads_has_a_line_for_the_menu_and_changes_something() {
        let golden = Look::of(LookName::GoldenHour);
        for (key, title, _) in BUILTIN {
            let file = LookPackFile::builtin(key).unwrap_or_else(|error| panic!("{key}: {error:#}"));
            assert!(!title.is_empty() && file.description.len() > 20, "{key} says what it is");
            let worn = file.worn(golden.sky, Some(golden.sun), golden.post).unwrap();
            let same = worn.post == golden.post
                && worn.sun == Some(golden.sun)
                && sky_data(&worn.sky) == sky_data(&golden.sky)
                && worn.voxels.is_none();
            assert!(!same, "{key} changes how a level looks");
        }
        assert!(LookPackFile::builtin("gloomy").is_err());
    }

    #[test]
    fn the_worn_look_follows_the_levels_lighting_and_goes_with_the_pack() {
        let mut world = World::new();
        update(&mut world);
        assert!(world.resource::<WornLook>().is_none(), "none worn");
        world.insert_resource(Look::of(LookName::GoldenHour).post);
        world.insert_resource(Wearing::new("test", pack("[post.bloom]\nintensity = 0.5\n[blocks]\nvoxels = 8")));
        update(&mut world);
        let worn = *world.resource::<WornLook>().expect("worked out");
        assert_eq!((worn.post.bloom.intensity, worn.voxels.map(|v| v.count)), (0.5, Some(8)));
        assert_eq!(worn.post.tonemap, Look::of(LookName::GoldenHour).post.tonemap);
        // The level's lighting changes under the pack: the worn look is worked out again over it.
        let night = Look::of(LookName::NightRaid);
        world.insert_resource(night.post);
        world.insert_resource(night.sky);
        update(&mut world);
        let worn = *world.resource::<WornLook>().unwrap();
        assert_eq!((worn.post.bloom.intensity, worn.post.tonemap), (0.5, night.post.tonemap));
        assert_eq!(sky_data(&worn.sky), sky_data(&night.sky));
        // Taken off, the level is drawn as it is.
        world.remove_resource::<Wearing>();
        update(&mut world);
        assert!(world.resource::<WornLook>().is_none());
    }

    #[test]
    fn a_players_folder_lists_its_packs_by_name_and_leaves_out_what_cannot_be_read() {
        let dir = tempfile::tempdir().unwrap();
        let looks = player_folder(&dir.path().join("Saves")).unwrap();
        assert_eq!(looks, dir.path().join(LOOKS_FOLDER));
        assert!(player_packs(&looks).is_empty(), "no folder yet");
        std::fs::create_dir_all(&looks).unwrap();
        std::fs::write(looks.join("Warm.lookpack"), "description = \"Warmer.\"\n[sun]\ntint = [1.0, 0.9, 0.8]")
            .unwrap();
        std::fs::write(looks.join("bright.lookpack"), "[post.bloom]\nintensity = 0.4").unwrap();
        std::fs::write(looks.join("Broken.lookpack"), "[post.bloom]\nglare = 1").unwrap();
        std::fs::write(looks.join("notes.txt"), "not a pack").unwrap();
        let packs = player_packs(&looks);
        let names: Vec<&str> = packs.iter().map(|pack| pack.name.as_str()).collect();
        assert_eq!(names, ["bright", "Warm"]);
        assert_eq!((packs[1].description.as_str(), packs[1].source), ("Warmer.", PackSource::Player));
        let path = packs[1].reference.strip_prefix(FILE_PREFIX).expect("named by its file");
        assert_eq!(Path::new(path), looks.join("Warm.lookpack"));
    }
}
