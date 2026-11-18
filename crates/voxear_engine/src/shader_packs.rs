//! Shader packs as a game wears them: where they come from, putting one
//! on, its options, and drawing it again as its files change. What a pack
//! is and how it is drawn is `voxear_render::shader_packs`.
//!
//! A pack comes from one of three places, as look packs do: the engine's
//! own, built in (`builtin:evening`); the game's, a `Name.shaderpack`
//! folder in its project, named by its `pack.toml`'s path or GUID; or the
//! player's, a folder in the game's `ShaderPacks` folder beside its saves
//! (`file:` and the folder's path). [`list`] gives them all, for a menu.
//!
//! [`wear`] reads a pack and compiles its programs before it returns, as
//! a render from the command line wants; [`wear_soon`] compiles them on a
//! thread, as the editor does, and the pack worn before stays until they
//! are done. [`update`], each tick, puts a pack on once it has compiled,
//! sets its options and the level's clock for its programs, and while its
//! files are [`watch`]ed compiles it again when one is saved. A pack that
//! does not compile is logged with Slang's message, naming the file and the
//! line, and the one drawn before carries on.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    thread::JoinHandle,
    time::{Duration, Instant, SystemTime},
};

use anyhow::{Context, Result, bail};
use voxear_assets::AssetKind;
use voxear_ecs::World;
use voxear_render::shader_packs::{
    self as packs, CompiledPack, MANIFEST, MAX_OPTIONS, OptionKind, PackFiles, PackManifest, PackOption, WornShaderPack,
};

use crate::{
    content::Content,
    look_packs::{PackInfo, PackSource},
};

/// The folder a player puts shader packs of their own in, beside the
/// game's saves.
pub const SHADER_PACKS_FOLDER: &str = "ShaderPacks";

/// What a pack read from a folder outside the game is named by, before its
/// path.
pub const FILE_PREFIX: &str = "file:";

/// How often a watched pack's files are looked at for changes.
const WATCH_EVERY: Duration = Duration::from_millis(500);

/// What a compile on a thread gives back: the manifest it was made from,
/// and the pack.
type Compiled = Result<(PackManifest, CompiledPack)>;

/// The shader pack the world wears, or is putting on, as a resource: what
/// names it, its manifest and files, its options' values, and how its last
/// compile went.
pub struct Wearing {
    pub reference: String,
    pub manifest: PackManifest,
    files: PackFiles,
    /// Each option's value, in the manifest's order.
    values: [f32; MAX_OPTIONS],
    compiling: Option<JoinHandle<Compiled>>,
    /// What the last compile said, when it failed.
    error: Option<String>,
    /// Each watched file's stamp, and when they were last looked at.
    watched: Option<(Vec<(PathBuf, SystemTime)>, Instant)>,
    /// The level's clock as last seen, and how many days it has turned.
    clock: Option<f32>,
    day: u32,
}

/// How a worn pack stands.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase", tag = "state", content = "error")]
pub enum Standing {
    /// Its programs are compiling; the pack drawn before, if any, is drawn.
    Compiling,
    /// It is drawn.
    Drawn,
    /// Its last compile failed, with Slang's message; the pack drawn before,
    /// if any, is drawn.
    Failed(String),
}

/// The pack `reference` names: `builtin:` and a name, `file:` and the path
/// of a folder outside the game (or its `pack.toml`), or a `pack.toml`
/// asset of the game's. A built-in pack is read from the engine's source
/// checkout when `live` and there is one, so its files can be watched.
pub fn load(content: &mut Content, reference: &str, live: bool) -> Result<(PackManifest, PackFiles)> {
    if let Some(name) = reference.strip_prefix("builtin:") {
        let files = match packs::builtin_folder(name).filter(|_| live) {
            Some(folder) => PackFiles::Folder(folder),
            None => PackFiles::Embedded(packs::builtin_files(name)?),
        };
        let manifest = PackManifest::parse(&files.read(MANIFEST)?)
            .with_context(|| format!("reading the built-in shader pack {name}"))?;
        return Ok((manifest, files));
    }
    if let Some(path) = reference.strip_prefix(FILE_PREFIX) {
        let path = Path::new(path);
        let folder = if path.is_dir() { path } else { path.parent().unwrap_or(path) };
        return read_folder(folder);
    }
    let guid = content.parse(reference)?.guid().with_context(|| format!("`{reference}` is not a shader pack"))?;
    if content.is_stripped(guid) {
        bail!("a server build draws no shader packs");
    }
    let (text, path) = content.asset_text(guid, AssetKind::ShaderPack)?;
    let manifest = PackManifest::parse(&text).with_context(|| format!("reading {path}"))?;
    // In the project, read from its folder, so Slang names its files; built, from the bundles its programs went in.
    match content.db().path(guid).filter(|file| file.is_file()) {
        Some(file) => Ok((manifest, PackFiles::Folder(file.parent().unwrap_or(Path::new(".")).to_path_buf()))),
        None => {
            let folder = path.rsplit_once('/').map_or("", |(folder, _)| folder);
            let mut programs = Vec::new();
            for pass in &manifest.passes {
                let named = format!("{folder}/{}", pass.program);
                let text = content.read_text(&named).with_context(|| format!("reading {named}"))?;
                if programs.iter().all(|(name, _): &(String, String)| *name != pass.program) {
                    programs.push((pass.program.clone(), text));
                }
            }
            Ok((manifest, PackFiles::Files(programs.into())))
        }
    }
}

fn read_folder(folder: &Path) -> Result<(PackManifest, PackFiles)> {
    let manifest = folder.join(MANIFEST);
    let text = std::fs::read_to_string(&manifest).with_context(|| format!("reading {}", manifest.display()))?;
    let parsed = PackManifest::parse(&text).with_context(|| format!("reading {}", manifest.display()))?;
    Ok((parsed, PackFiles::Folder(folder.to_path_buf())))
}

/// Wears the pack `reference` names, compiled before this returns, from the
/// next frame; with an empty reference none. A pack that cannot be read or
/// compiled leaves the one worn as it is.
pub fn wear(world: &mut World, content: &mut Content, reference: &str) -> Result<()> {
    if reference.is_empty() {
        take_off(world);
        return Ok(());
    }
    let (manifest, files) = load(content, reference, false).with_context(|| format!("the shader pack {reference}"))?;
    let compiled =
        packs::compile(&manifest, &files).with_context(|| format!("compiling the shader pack {reference}"))?;
    let values = carried_over(world, &manifest);
    let mut wearing = Wearing::new(lasting(content, reference), manifest, files);
    wearing.values = values;
    world.insert_resource(wearing);
    put_on(world, compiled);
    Ok(())
}

/// Starts wearing the pack `reference` names: its programs compile on a
/// thread, and [`update`] puts it on once they have; the pack worn before
/// is drawn until then. `live` watches its files, as the editor does, and
/// compiles it again as one is saved. An empty reference takes the pack off
/// at once.
pub fn wear_soon(world: &mut World, content: &mut Content, reference: &str, live: bool) -> Result<()> {
    if reference.is_empty() {
        take_off(world);
        return Ok(());
    }
    let (manifest, files) = load(content, reference, live).with_context(|| format!("the shader pack {reference}"))?;
    let values = carried_over(world, &manifest);
    let mut wearing = Wearing::new(lasting(content, reference), manifest.clone(), files.clone());
    wearing.values = values;
    wearing.compiling = Some(std::thread::spawn(move || compile_owned(manifest, files)));
    if live {
        wearing.watched = Some((stamps(&wearing.files), Instant::now()));
    }
    world.insert_resource(wearing);
    Ok(())
}

impl Wearing {
    fn new(reference: String, manifest: PackManifest, files: PackFiles) -> Self {
        let values = manifest.default_values();
        Self { reference, manifest, files, values, compiling: None, error: None, watched: None, clock: None, day: 0 }
    }
}

fn compile_owned(manifest: PackManifest, files: PackFiles) -> Compiled {
    let compiled = packs::compile(&manifest, &files)?;
    Ok((manifest, compiled))
}

/// The options' values a pack worn again keeps, by their names, and the
/// rest at the pack's defaults.
fn carried_over(world: &World, manifest: &PackManifest) -> [f32; MAX_OPTIONS] {
    let mut values = manifest.default_values();
    let Some(before) = world.resource::<Wearing>() else { return values };
    for (at, option) in manifest.options.iter().enumerate() {
        let kept = before.manifest.options.iter().position(|old| old.key == option.key && same_kind(old, option));
        if let Some(old) = kept {
            values[at] = option.kept(before.values[old]);
        }
    }
    values
}

fn same_kind(a: &PackOption, b: &PackOption) -> bool {
    std::mem::discriminant(&a.kind) == std::mem::discriminant(&b.kind)
}

/// Draws a compiled pack from the next frame, with the options worn.
fn put_on(world: &mut World, compiled: CompiledPack) {
    let Some(wearing) = world.resource::<Wearing>() else { return };
    let worn = WornShaderPack {
        options: wearing.values,
        time_of_day: wearing.clock.unwrap_or(12.0),
        day: wearing.day,
        ..WornShaderPack::new(Arc::new(compiled))
    };
    world.insert_resource(worn);
}

/// Wears in `to` the pack `from` wears, as it is drawn there, with its
/// options: as the editor's Scene shows its pack in the game it plays.
pub fn carry(from: &World, to: &mut World) {
    let Some(wearing) = from.resource::<Wearing>() else { return };
    let mut carried = Wearing::new(wearing.reference.clone(), wearing.manifest.clone(), wearing.files.clone());
    carried.values = wearing.values;
    carried.watched = wearing.watched.as_ref().map(|_| (stamps(&carried.files), Instant::now()));
    to.insert_resource(carried);
    if let Some(worn) = from.resource::<WornShaderPack>() {
        to.insert_resource(worn.clone());
    }
}

/// Takes the pack worn off: the frame is drawn as the engine draws it.
pub fn take_off(world: &mut World) {
    world.remove_resource::<Wearing>();
    world.remove_resource::<WornShaderPack>();
}

/// What names the pack worn; none for none.
pub fn worn(world: &World) -> Option<&str> {
    world.resource::<Wearing>().map(|wearing| wearing.reference.as_str())
}

/// How the pack worn stands; none for none.
pub fn standing(world: &World) -> Option<Standing> {
    let wearing = world.resource::<Wearing>()?;
    Some(match (&wearing.compiling, &wearing.error) {
        (Some(_), _) => Standing::Compiling,
        (None, Some(error)) => Standing::Failed(error.clone()),
        (None, None) => Standing::Drawn,
    })
}

/// The pack worn's options, each with its value now.
pub fn options(world: &World) -> Vec<(PackOption, f32)> {
    let Some(wearing) = world.resource::<Wearing>() else { return Vec::new() };
    wearing.manifest.options.iter().cloned().zip(wearing.values).collect()
}

/// Sets one of the pack worn's options by its name, kept to what it can be,
/// and gives the value set: a number, a toggle as 0 or 1, a choice as its
/// place among the choices.
pub fn set_option(world: &mut World, key: &str, value: f32) -> Result<f32> {
    let Some(wearing) = world.resource_mut::<Wearing>() else { bail!("no shader pack is worn") };
    let Some(at) = wearing.manifest.options.iter().position(|option| option.key == key) else {
        let keys: Vec<&str> = wearing.manifest.options.iter().map(|option| option.key.as_str()).collect();
        bail!(
            "the shader pack has no option `{key}`; it has {}",
            if keys.is_empty() { "none".into() } else { keys.join(", ") }
        );
    };
    let kept = wearing.manifest.options[at].kept(value);
    wearing.values[at] = kept;
    let values = wearing.values;
    if let Some(worn) = world.resource_mut::<WornShaderPack>() {
        worn.options = values;
    }
    Ok(kept)
}

/// A choice option's value by the choice's name, for one set from a menu
/// or the command line.
pub fn choice_value(option: &PackOption, choice: &str) -> Option<f32> {
    match &option.kind {
        OptionKind::Choice { choices, .. } => {
            choices.iter().position(|named| named.eq_ignore_ascii_case(choice)).map(|at| at as f32)
        }
        _ => None,
    }
}

/// Watches the worn pack's files, as the editor does, and compiles it again
/// when one is saved; or stops. A built-in pack is read again from the
/// engine's source checkout, where there is one, to be watched.
pub fn watch(world: &mut World, on: bool) {
    let Some(wearing) = world.resource_mut::<Wearing>() else { return };
    if wearing.watched.is_some() == on {
        return;
    }
    if !on {
        wearing.watched = None;
        return;
    }
    if let Some(name) = wearing.reference.strip_prefix("builtin:")
        && let Some(folder) = packs::builtin_folder(name)
    {
        wearing.files = PackFiles::Folder(folder);
    }
    wearing.watched = Some((stamps(&wearing.files), Instant::now()));
}

/// Each file of a pack read from a folder, and when it was last saved.
fn stamps(files: &PackFiles) -> Vec<(PathBuf, SystemTime)> {
    let Some(folder) = files.folder() else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut stamps: Vec<(PathBuf, SystemTime)> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "slang" || e == "toml"))
        .filter_map(|path| Some((path.clone(), path.metadata().ok()?.modified().ok()?)))
        .collect();
    stamps.sort();
    stamps
}

/// Each tick: puts the pack on once it has compiled, or logs why it did
/// not; compiles it again when a watched file was saved; and gives its
/// programs the level's clock.
pub fn update(world: &mut World) {
    let climate = crate::heat::climate(world);
    let Some(wearing) = world.resource_mut::<Wearing>() else {
        if world.resource::<WornShaderPack>().is_some() {
            world.remove_resource::<WornShaderPack>();
        }
        return;
    };
    // The level's clock, and the days it has turned past midnight.
    let hour = climate.map_or(12.0, |climate| climate.time_of_day);
    if wearing.clock.is_some_and(|before| hour + 12.0 < before) {
        wearing.day += 1;
    }
    wearing.clock = Some(hour);
    let (day, values) = (wearing.day, wearing.values);
    // A saved file starts another compile, after the one under way.
    if wearing.compiling.is_none()
        && let Some((seen, last)) = &mut wearing.watched
        && last.elapsed() >= WATCH_EVERY
    {
        *last = Instant::now();
        let now = stamps(&wearing.files);
        if now != *seen {
            *seen = now;
            match wearing.files.folder().map(read_folder) {
                Some(Ok((manifest, files))) => {
                    tracing::info!("the shader pack {} changed: compiling it again", wearing.reference);
                    wearing.compiling = Some(std::thread::spawn(move || compile_owned(manifest, files)));
                }
                Some(Err(error)) => {
                    tracing::error!("the shader pack {}: {error:#}", wearing.reference);
                    wearing.error = Some(format!("{error:#}"));
                }
                None => {}
            }
        }
    }
    let done = wearing.compiling.as_ref().is_some_and(JoinHandle::is_finished);
    if done {
        let job = wearing.compiling.take().expect("checked");
        let compiled = job.join().unwrap_or_else(|_| Err(anyhow::anyhow!("the shader compiler thread panicked")));
        let reference = wearing.reference.clone();
        match compiled {
            Ok((manifest, compiled)) => {
                wearing.error = None;
                let values = remapped(&wearing.manifest, &manifest, wearing.values);
                (wearing.manifest, wearing.values) = (manifest, values);
                tracing::info!("the shader pack {reference} is drawn");
                put_on(world, compiled);
            }
            Err(error) => {
                tracing::error!("the shader pack {reference} did not compile; the frame is drawn as before: {error:#}");
                wearing.error = Some(format!("{error:#}"));
            }
        }
    }
    if let Some(worn) = world.resource_mut::<WornShaderPack>() {
        (worn.time_of_day, worn.day, worn.options) = (hour, day, values);
    }
}

/// Options' values set for `before`, laid out as `after` lists its options:
/// by their names, the rest at their defaults.
fn remapped(before: &PackManifest, after: &PackManifest, values: [f32; MAX_OPTIONS]) -> [f32; MAX_OPTIONS] {
    let mut kept = after.default_values();
    for (at, option) in after.options.iter().enumerate() {
        if let Some(old) = before.options.iter().position(|old| old.key == option.key && same_kind(old, option)) {
            kept[at] = option.kept(values[old]);
        }
    }
    kept
}

/// Waits for the worn pack's compile under way, if there is one, and puts
/// it on: for a render from the command line or a test.
pub fn finish(world: &mut World) {
    let Some(wearing) = world.resource_mut::<Wearing>() else { return };
    let Some(job) = wearing.compiling.as_ref() else { return };
    while !job.is_finished() {
        std::thread::sleep(Duration::from_millis(10));
    }
    update(world);
}

/// What names a pack for as long as it is there: a game's by its GUID,
/// which a new name or place for its folder leaves as it is, and any other
/// as it is named.
fn lasting(content: &Content, reference: &str) -> String {
    if reference.starts_with("builtin:") || reference.starts_with(FILE_PREFIX) {
        return reference.to_string();
    }
    let guid = content.parse(reference).ok().and_then(|asset| asset.guid());
    guid.map_or_else(|| reference.to_string(), |guid| guid.to_string())
}

/// Where a player's own packs are kept for a game whose saves are in
/// `saves`: `ShaderPacks`, beside them.
pub fn player_folder(saves: &Path) -> Option<PathBuf> {
    Some(saves.parent()?.join(SHADER_PACKS_FOLDER))
}

/// Every pack there is to wear: the engine's, then the game's by name, then
/// those in the player's `folder` by name. A player's pack that cannot be
/// read is left out, with a warning.
pub fn list(content: &mut Content, folder: Option<&Path>) -> Vec<PackInfo> {
    let mut found: Vec<PackInfo> = packs::BUILTIN
        .iter()
        .map(|(key, title, _)| PackInfo {
            reference: format!("builtin:{key}"),
            name: title.to_string(),
            description: PackManifest::builtin(key).map(|manifest| manifest.description).unwrap_or_default(),
            source: PackSource::Builtin,
        })
        .collect();
    let named = |path: &str| {
        let folder = Path::new(path).parent().and_then(Path::file_stem);
        folder.map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default()
    };
    let assets: Vec<_> = content
        .db()
        .entries()
        .filter(|entry| entry.kind == AssetKind::ShaderPack)
        .map(|entry| (entry.guid, named(&entry.path)))
        .collect();
    let mut game = Vec::new();
    for (guid, name) in assets {
        if content.is_stripped(guid) {
            continue;
        }
        let read = content.asset_text(guid, AssetKind::ShaderPack).ok();
        let description = read.and_then(|(text, _)| PackManifest::parse(&text).ok()).map(|m| m.description);
        let description = description.unwrap_or_default();
        game.push(PackInfo { reference: guid.to_string(), name, description, source: PackSource::Game });
    }
    game.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    found.extend(game);
    found.extend(folder.map(player_packs).unwrap_or_default());
    found
}

/// The packs in a player's `folder`: each a `Name.shaderpack` folder in it,
/// by name.
fn player_packs(folder: &Path) -> Vec<PackInfo> {
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut found = Vec::new();
    for path in entries.flatten().map(|entry| entry.path()) {
        if !path.is_dir() || path.extension().is_none_or(|e| e != packs::PACK_EXTENSION) {
            continue;
        }
        match read_folder(&path) {
            Ok((manifest, _)) => found.push(PackInfo {
                reference: format!("{FILE_PREFIX}{}", path.display()),
                name: path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default(),
                description: manifest.description,
                source: PackSource::Player,
            }),
            Err(error) => tracing::warn!("the shader pack {} is left out: {error:#}", path.display()),
        }
    }
    found.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    found
}

/// An option as tools read it: its name, label and tooltip, what it is, and
/// its value now.
pub fn option_json(option: &PackOption, value: f32) -> serde_json::Value {
    let mut json = serde_json::to_value(option).unwrap_or_default();
    json["value"] = match &option.kind {
        OptionKind::Toggle { .. } => serde_json::Value::Bool(value >= 0.5),
        OptionKind::Choice { choices, .. } => choices.get(value as usize).cloned().unwrap_or_default().into(),
        OptionKind::Number { .. } => serde_json::json!(value),
    };
    json
}

/// The pack worn as tools read it: what names it, how it stands, its passes
/// and its options.
pub fn worn_json(world: &World) -> serde_json::Value {
    let Some(wearing) = world.resource::<Wearing>() else { return serde_json::Value::Null };
    let manifest = &wearing.manifest;
    let passes: Vec<serde_json::Value> = manifest
        .passes
        .iter()
        .map(|pass| {
            let writes = match pass.writes {
                packs::PassWrites::Color => "color".to_string(),
                packs::PassWrites::Target(target) => manifest.targets[target].name.clone(),
            };
            serde_json::json!({
                "name": pass.name,
                "program": pass.program,
                "at": pass.point.key(),
                "reads": pass.reads.names(manifest),
                "writes": writes,
            })
        })
        .collect();
    let options: Vec<serde_json::Value> =
        options(world).iter().map(|(option, value)| option_json(option, *value)).collect();
    let surfaces: serde_json::Map<String, serde_json::Value> =
        manifest.surfaces.named().into_iter().map(|(key, program)| (key.to_string(), program.into())).collect();
    serde_json::json!({
        "reference": wearing.reference,
        "description": manifest.description,
        "standing": standing(world),
        "passes": passes,
        "surfaces": surfaces,
        "shadows": shadows_json(&manifest.shadows),
        "options": options,
    })
}

/// What a pack sets of the sun's shadows, only what it sets.
fn shadows_json(shadows: &packs::PackShadows) -> serde_json::Value {
    let mut json = serde_json::Map::new();
    if let Some(filter) = &shadows.filter {
        json.insert("filter".into(), filter.clone().into());
    }
    if let Some(softness) = shadows.softness {
        json.insert("softness".into(), softness.into());
    }
    if let Some(distance) = shadows.distance {
        json.insert("distance".into(), distance.into());
    }
    if let Some(tint) = shadows.tint {
        json.insert("tint".into(), tint.to_vec().into());
    }
    if let Some(maps) = shadows.maps {
        json.insert("maps".into(), maps.name().into());
    }
    if shadows.leaves {
        json.insert("leaves".into(), true.into());
    }
    json.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_players_folder_lists_its_packs_by_name_and_leaves_out_what_cannot_be_read() {
        let dir = tempfile::tempdir().unwrap();
        let folder = player_folder(&dir.path().join("Saves")).unwrap();
        assert_eq!(folder, dir.path().join(SHADER_PACKS_FOLDER));
        assert!(player_packs(&folder).is_empty(), "no folder yet");
        let pass = "[[pass]]\nname = \"A\"\nprogram = \"a.slang\"\nat = \"final\"\n";
        for (name, manifest) in [("Warm", format!("description = \"Warmer.\"\n{pass}")), ("bright", pass.into())] {
            let pack = folder.join(format!("{name}.shaderpack"));
            std::fs::create_dir_all(&pack).unwrap();
            std::fs::write(pack.join(MANIFEST), manifest).unwrap();
        }
        let broken = folder.join("Broken.shaderpack");
        std::fs::create_dir_all(&broken).unwrap();
        std::fs::write(broken.join(MANIFEST), "format = 1").unwrap();
        std::fs::create_dir_all(folder.join("Notes")).unwrap();
        let found = player_packs(&folder);
        let names: Vec<&str> = found.iter().map(|pack| pack.name.as_str()).collect();
        assert_eq!(names, ["bright", "Warm"]);
        assert_eq!((found[1].description.as_str(), found[1].source), ("Warmer.", PackSource::Player));
        let path = found[1].reference.strip_prefix(FILE_PREFIX).expect("named by its folder");
        assert_eq!(Path::new(path), folder.join("Warm.shaderpack"));
    }

    #[test]
    fn options_set_are_kept_to_what_they_can_be_and_carry_over_by_name() {
        let text = |extra: &str| {
            format!(
                "[options]\nstrength = {{ kind = \"number\", range = [0.0, 2.0], default = 1.0 }}\n{extra}\
                 [[pass]]\nname = \"A\"\nprogram = \"a.slang\"\nat = \"final\"\n"
            )
        };
        let before = PackManifest::parse(&text(
            "look = { kind = \"choice\", choices = [\"Soft\", \"Hard\"], default = \"Soft\" }\n",
        ))
        .unwrap();
        let after = PackManifest::parse(&text("glow = { kind = \"toggle\", default = true }\n")).unwrap();
        let mut values = before.default_values();
        values[0] = 1.7;
        values[1] = 1.0;
        let kept = remapped(&before, &after, values);
        assert_eq!(kept[..2], [1.7, 1.0], "strength kept, the new toggle at its default");
        assert_eq!(choice_value(&before.options[1], "hard"), Some(1.0));
        assert_eq!(choice_value(&before.options[0], "hard"), None);
        let json = option_json(&before.options[1], 1.0);
        assert_eq!((json["kind"].as_str(), json["value"].as_str()), (Some("choice"), Some("Hard")));
    }
}
