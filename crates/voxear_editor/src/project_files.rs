//! What the Project panel does to the project's files. New folders, scenes,
//! levels, materials and C# scripts get names not yet taken ("New Scene 1").
//! Renaming takes an asset's `.meta` along, so its GUID and every reference
//! to it hold, and renaming a script renames the class it declares, which
//! must match the file. Duplicating copies the file without its `.meta`, so
//! the copy gets a GUID of its own. Deleting moves a file and its `.meta`
//! into the project's `.voxear/Trash`, where it can be got back, rather than
//! destroying it. Files dropped on the panel from outside the editor are
//! copied in, numbered when their names are taken.
//!
//! The panel's paths are the content's: a folder package's files are under
//! `Packages/<name>/`, and change in the package's own folder (see
//! [`place`]). A package from the machine's cache never changes.

use voxear_terrain::file::TerrainFile;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use voxear_assets::{Access, Mount, Mounts, PACKAGES_FOLDER};
use voxear_engine::shaders::BuiltinShader;

/// Where a path of the Project panel's changes on disk: the folder of its
/// mount, the project's or a folder package's, and its path in there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    pub root: PathBuf,
    pub path: String,
    pub mount: Mount,
}

impl Place {
    /// A path in the same mount, from one within it, as the functions here
    /// return, back as the panel names it.
    pub fn back(&self, local: &str) -> String {
        self.mount.content_path(local)
    }
}

/// Where `path` changes, or why it cannot: a package from the machine's
/// cache is shared by every project on it, and `Packages` itself only
/// holds the packages.
pub fn place(mounts: &Mounts, path: &str) -> Result<Place> {
    let (mount, local) = mounts.locate(path);
    if mount == Mount::Project && path == PACKAGES_FOLDER && !mounts.packages().is_empty() {
        bail!("{PACKAGES_FOLDER} holds the project's packages; add them with the Package Manager");
    }
    if mounts.access(&mount) == Access::ReadOnly {
        let title = mount.package().and_then(|name| mounts.package(name)).map_or(path, |p| p.title.as_str());
        bail!("{title} is read-only: it is in the package cache");
    }
    let root = mounts.folder(&mount).context("the package is not mounted")?.to_path_buf();
    Ok(Place { root, path: local.to_string(), mount })
}

/// Where a file or folder to rename, copy, delete or move changes: as
/// [`place`], but never a package's own folder, which the Package Manager
/// adds and removes.
pub fn item_place(mounts: &Mounts, path: &str) -> Result<Place> {
    let place = place(mounts, path)?;
    let package = place.mount != Mount::Project && place.path.is_empty();
    ensure!(!package, "packages are added and removed in the Package Manager");
    Ok(place)
}

/// Where a file or folder and the folder it moves into both change, which
/// must be one mount: the project's, or one folder package's.
pub fn move_place(mounts: &Mounts, path: &str, folder: &str) -> Result<(Place, String)> {
    let (from, into) = (item_place(mounts, path)?, place(mounts, folder)?);
    ensure!(from.mount == into.mount, "files move within the project, or within one package");
    Ok((from, into.path))
}

/// New lighting settings: a look to start from, and where its settings go.
const NEW_LIGHTING: &str = "# Voxear lighting settings. A scene names them, lighting = \"<this file>\",
# and scripts apply them with Lighting.Apply.
format = 1

# The look to start from, one of the built-in looks (the Lighting page lists them), such as golden_hour,
# night_raid or candlelit_castle. Change anything below it.
look = \"golden_hour\"

# The sky, as a scene's [sky] writes it: mode, haze, haze_tint, haze_glow, distance_haze, sun_size, ambient, stars.
[sky]

# How the sun looks: temperature (kelvin), tint, strength, softness, shadow_tint.
[sun]

# Post-processing, as a scene's [post] writes it: [post.exposure], [post.bloom], [post.grading], [post.fog] and more.
[post]
";

/// Something the Project panel makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewFile {
    Folder,
    Scene,
    Level,
    Material,
    Lighting,
    Shader,
    UnlitShader,
    /// A shader a UI element's background is drawn with.
    UiShader,
    Script,
    MotionSet,
    GunHandling,
    Terrain,
    /// A world generator, which a terrain's Generate layer picks.
    WorldGen,
    /// Pictures prompts show for controls, picked on the Input page.
    PromptIcons,
}

impl NewFile {
    pub const ALL: [NewFile; 14] = [
        NewFile::Folder,
        NewFile::Scene,
        NewFile::Level,
        NewFile::Material,
        NewFile::Lighting,
        NewFile::Shader,
        NewFile::UnlitShader,
        NewFile::UiShader,
        NewFile::Script,
        NewFile::MotionSet,
        NewFile::GunHandling,
        NewFile::Terrain,
        NewFile::WorldGen,
        NewFile::PromptIcons,
    ];

    /// The module it belongs to, without which the menu leaves it out.
    pub fn module(self) -> Option<voxear_engine::modules::Module> {
        match self {
            NewFile::GunHandling => Some(voxear_engine::modules::Module::Guns),
            NewFile::Terrain | NewFile::WorldGen => Some(voxear_engine::modules::Module::Terrain),
            _ => None,
        }
    }

    /// As the menu offers it.
    pub fn label(self) -> &'static str {
        match self {
            NewFile::Folder => "New Folder",
            NewFile::Scene => "New Scene",
            NewFile::Level => "New Level",
            NewFile::Material => "New Material",
            NewFile::Lighting => "New Lighting Settings",
            NewFile::Shader => "New Shader",
            NewFile::UnlitShader => "New Unlit Shader",
            NewFile::UiShader => "New UI Shader",
            NewFile::Script => "New C# Script",
            NewFile::MotionSet => "New Motion Set",
            NewFile::GunHandling => "New Gun Handling",
            NewFile::Terrain => "New Terrain",
            NewFile::WorldGen => "New World Generator",
            NewFile::PromptIcons => "New Prompt Icons",
        }
    }

    /// The name it is made with, before a number makes it unique, and its
    /// extension.
    fn name(self) -> (&'static str, &'static str) {
        match self {
            NewFile::Folder => ("New Folder", ""),
            NewFile::Scene => ("New Scene", ".scene"),
            NewFile::Level => ("New Level", ".level"),
            NewFile::Material => ("New Material", ".material"),
            NewFile::Lighting => ("New Lighting", ".lighting"),
            NewFile::Shader => ("New Shader", ".shader"),
            NewFile::UnlitShader => ("New Unlit Shader", ".shader"),
            NewFile::UiShader => ("New UI Shader", ".shader"),
            NewFile::Script => ("NewBehaviour", ".cs"),
            NewFile::MotionSet => ("New Motion Set", ".motionset"),
            NewFile::GunHandling => ("New Gun Handling", ".gunhandling"),
            NewFile::Terrain => ("New Terrain", ".terrain"),
            NewFile::WorldGen => ("New World Generator", ".worldgen"),
            NewFile::PromptIcons => ("New Prompt Icons", ".prompticons"),
        }
    }
}

/// Makes `kind` in `folder` (from the project's folder) and returns its path
/// from the project's folder. A script's class is in `namespace`.
pub fn create(root: &Path, folder: &str, kind: NewFile, namespace: &str) -> Result<String> {
    let (stem, extension) = kind.name();
    let name = unique(&root.join(folder), stem, extension);
    let path = join(folder, &name);
    let file = root.join(&path);
    match kind {
        NewFile::Folder => std::fs::create_dir_all(&file),
        NewFile::Scene => std::fs::write(&file, "# Voxear scene\nformat = 1\n"),
        NewFile::Level => std::fs::write(&file, "# Voxear level: list its scenes as [[scene]] tables.\nformat = 1\n"),
        NewFile::Material => std::fs::write(&file, "color = [0.8, 0.8, 0.8]\nmetallic = 0.0\nroughness = 0.5\n"),
        NewFile::Lighting => std::fs::write(&file, NEW_LIGHTING),
        NewFile::Shader => std::fs::write(&file, shader(NEW_SHADER, &name[..name.len() - extension.len()])),
        NewFile::UnlitShader => std::fs::write(&file, shader(NEW_UNLIT_SHADER, &name[..name.len() - extension.len()])),
        NewFile::UiShader => std::fs::write(&file, shader(NEW_UI_SHADER, &name[..name.len() - extension.len()])),
        NewFile::Script => std::fs::write(&file, script(namespace, &name[..name.len() - extension.len()])),
        NewFile::MotionSet => std::fs::write(&file, NEW_MOTION_SET),
        NewFile::GunHandling => std::fs::write(&file, NEW_GUN_HANDLING),
        NewFile::Terrain => std::fs::write(&file, NEW_TERRAIN.to_string() + &TerrainFile::default().to_text()),
        NewFile::WorldGen => std::fs::write(&file, new_world_gen()),
        NewFile::PromptIcons => {
            let icons = voxear_engine::prompt_icons::PromptIconsFile { format: 1, ..Default::default() };
            std::fs::write(&file, icons.to_text()?)
        }
    }
    .with_context(|| format!("making {}", file.display()))?;
    Ok(path)
}

/// What a new terrain's file starts with, before its settings: flat ground,
/// its heights written beside it once it is shaped and the level saved.
const NEW_TERRAIN: &str = "# Voxear terrain: how large and how fine the ground is; its heights are in the .heights file beside it.\n";

/// What a new world generator's file holds: a heading, then Rolling Hills.
pub(crate) fn new_world_gen() -> String {
    NEW_WORLD_GEN.to_string() + &voxear_terrain::generate::WorldGen::default().to_text()
}

/// What a world generator's file starts with, before its settings.
pub(crate) const NEW_WORLD_GEN: &str =
    "# Voxear world generator: a seed and how it shapes ground, which a terrain's Generate layer picks.\n";

/// Makes a new world generator in `folder` (from the project's folder),
/// named `name` and a number if that is taken, and returns its path.
pub fn create_world_gen(root: &Path, folder: &str, name: &str) -> Result<String> {
    std::fs::create_dir_all(root.join(folder)).with_context(|| format!("making {folder}"))?;
    let file = unique(&root.join(folder), name, ".worldgen");
    let path = join(folder, &file);
    std::fs::write(root.join(&path), new_world_gen()).with_context(|| format!("making {path}"))?;
    Ok(path)
}

/// Makes a new terrain asset in `folder` (from the project's folder), named
/// `name` and a number if that is taken, and returns its path.
pub fn create_terrain(root: &Path, folder: &str, name: &str) -> Result<String> {
    std::fs::create_dir_all(root.join(folder)).with_context(|| format!("making {folder}"))?;
    let named: String = name.chars().filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')).collect();
    let stem = if named.trim().is_empty() { "Terrain" } else { named.trim() };
    let file = unique(&root.join(folder), stem, ".terrain");
    let path = join(folder, &file);
    let text = NEW_TERRAIN.to_string() + &TerrainFile::default().to_text();
    std::fs::write(root.join(&path), text).with_context(|| format!("making {path}"))?;
    Ok(path)
}

/// A new Motion Set: a state to walk about in, its clips to be picked.
const NEW_MOTION_SET: &str = "# Voxear Motion Set: what a character can do, its states' looping clips and its actions.\n[[state]]\nname = \"Walking\"\nrole = \"grounded\"\n";

/// A new Gun Handling: the gun held and its actions played as its kind's
/// built-ins have them, until its Inspector bases them on clips.
pub(crate) const NEW_GUN_HANDLING: &str =
    "# Voxear Gun Handling: how a gun is held and how its actions go, as offsets on its kind's built-ins.\n";

/// A new shader, its struct named for its file: the file's name without
/// what a name in code cannot have.
fn shader(template: &str, name: &str) -> String {
    template.replace("NAME", &struct_name(name))
}

/// A shader file's name as its struct's: without what a name in code
/// cannot have.
fn struct_name(name: &str) -> String {
    let named: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    if named.starts_with(|c: char| c.is_ascii_alphabetic()) { named } else { format!("Shader{named}") }
}

/// What a new shader starts as: a lit, an unlit or a UI one to write, or a
/// copy of one built in, to change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShaderStart {
    Lit,
    Unlit,
    Ui,
    Copy(BuiltinShader),
}

impl ShaderStart {
    /// As New Shader offers them: the three to write first, then the built-in ones.
    pub fn all() -> Vec<Self> {
        let mut all = vec![Self::Lit, Self::Unlit, Self::Ui];
        all.extend(BuiltinShader::ALL.map(Self::Copy));
        all
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Lit => "Lit",
            Self::Unlit => "Unlit",
            Self::Ui => "UI",
            Self::Copy(builtin) => builtin.title(),
        }
    }

    /// What it starts as, beside its name.
    pub fn detail(self) -> &'static str {
        match self {
            Self::Lit | Self::Unlit | Self::Ui => "To write",
            Self::Copy(_) => "Built in, to change",
        }
    }
}

/// Makes a new shader in `folder` (from the project's folder) as `start`
/// says, and returns its path.
pub fn create_shader(root: &Path, folder: &str, start: ShaderStart, namespace: &str) -> Result<String> {
    match start {
        ShaderStart::Lit => create(root, folder, NewFile::Shader, namespace),
        ShaderStart::Unlit => create(root, folder, NewFile::UnlitShader, namespace),
        ShaderStart::Ui => create(root, folder, NewFile::UiShader, namespace),
        ShaderStart::Copy(builtin) => create_shader_from(root, folder, builtin),
    }
}

/// Makes a copy of a built-in shader in `folder` (from the project's
/// folder), to change as the project's own, and returns its path. Its
/// struct is named for its file, as a new shader's is.
fn create_shader_from(root: &Path, folder: &str, builtin: BuiltinShader) -> Result<String> {
    let name = unique(&root.join(folder), builtin.title(), ".shader");
    let path = join(folder, &name);
    let named = struct_name(&name[..name.len() - ".shader".len()]);
    let source = replace_word(builtin.source(), builtin.title(), &named);
    let file = root.join(&path);
    std::fs::write(&file, source).with_context(|| format!("making {}", file.display()))?;
    Ok(path)
}

/// `text` with `word` replaced by `with` wherever it stands as a whole
/// word, not inside another.
fn replace_word(text: &str, word: &str, with: &str) -> String {
    let in_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(word) {
        let previous = if at == 0 { out.chars().last() } else { rest[..at].chars().last() };
        let before = previous.is_some_and(in_word);
        let after = rest[at + word.len()..].chars().next().is_some_and(in_word);
        out.push_str(&rest[..at]);
        out.push_str(if before || after { word } else { with });
        rest = &rest[at + word.len()..];
    }
    out.push_str(rest);
    out
}

/// What a new shader starts as: a lit surface with a tint, a gloss and a
/// pattern, and a vertex and a light function to uncomment.
const NEW_SHADER: &str = r#"// A custom shader. Materials pick it as their Shader, and set its properties in their
// Inspector. The Manual's Custom Shaders page says everything a shader can do.

struct NAME
{
    [Property, Color] float3 tint = float3(1.0, 1.0, 1.0);
    [Property, Range(0, 1), Tooltip("How glossy it is: 1 is a mirror")] float gloss = 0.5;
    [Property] Picture pattern;
}

// The surface, which starts as the material's own colour, normal, metallic, roughness and emission.
void surface(NAME shader, SurfaceInput input, inout SurfaceOutput surface)
{
    surface.color *= shader.tint * shader.pattern.sample(input.uv).rgb;
    surface.roughness = 1.0 - shader.gloss;
}

// Uncomment to move vertices: how far, in metres, in the world. Say how far at most with
// [MaxOffset(metres)] on the struct.
// float3 vertex(NAME shader, VertexInput input)
// {
//     return float3(0.0, sin(input.time * 2.0 + input.position.x) * 0.1, 0.0);
// }

// Uncomment to light it your own way: the light from the sun or a lamp, as it falls on the surface.
// float3 light(NAME shader, LightInput light, SurfaceOutput surface)
// {
//     float lit = dot(surface.normal, light.direction) > 0.2 ? 1.0 : 0.3;
//     return surface.color / PI * light.color * lit;
// }
"#;

/// What a new unlit shader starts as: a colour and how bright it is.
const NEW_UNLIT_SHADER: &str = r#"// An unlit shader: drawn as its colour, the same in light and in shadow, as a screen,
// a glow or a sky. Materials pick it as their Shader.

[Unlit]
struct NAME
{
    [Property, Color] float3 color = float3(1.0, 1.0, 1.0);
    [Property, Range(0, 20)] float brightness = 1.0;
}

void surface(NAME shader, SurfaceInput input, inout SurfaceOutput surface)
{
    surface.color = shader.color * shader.brightness;
    surface.emission = 0.0;
}
"#;

/// What a new UI shader starts as: a band of light that sweeps across the
/// element's background, in a colour, at a speed.
const NEW_UI_SHADER: &str = r#"// A UI shader. A UI element's background is drawn with it when its style says
// shader: url("Shaders/NAME.shader"), and its properties are the element's custom properties of the
// same names, as --glow: #8b93ff. The Manual's UI Shaders page says everything a UI shader can do.

struct NAME
{
    [Property, Color] float4 glow = float4(1.0, 1.0, 1.0, 0.4);
    [Property, Range(0, 4), Tooltip("How many times a second the light crosses")] float speed = 0.5;
}

// The background's colour at one pixel, premultiplied by its alpha. The border, rounded corners and
// opacity go on after.
float4 ui(NAME shader, UiInput input)
{
    float at = frac(input.time * shader.speed) * 1.6 - 0.3;
    float band = saturate(1.0 - abs(input.uv.x - at) / 0.3);
    float4 light = shader.glow * band * band;
    return light + input.background * (1.0 - light.a);
}
"#;

/// Renames a file or folder in place, with its `.meta`, and returns its new
/// path. A file keeps its extension, and a script's class follows its name.
pub fn rename(root: &Path, path: &str, name: &str) -> Result<String> {
    let name = name.trim();
    ensure!(!name.is_empty() && !name.contains(['/', '\\']), "a name cannot be empty or have slashes in it");
    let from = root.join(path);
    // A file keeps its kind, as in Unity's Project window: a name typed
    // without its extension gets it back.
    let extension = split_extension(path.rsplit('/').next().unwrap_or(path)).1;
    let name = match from.is_file() && !extension.is_empty() && !name.ends_with(extension) {
        true => format!("{name}{extension}"),
        false => name.to_string(),
    };
    let name = name.as_str();
    let folder = parent(path);
    let renamed = join(folder, name);
    let to = root.join(&renamed);
    if renamed == path {
        return Ok(renamed);
    }
    ensure!(!to.exists(), "{name} is already there");
    std::fs::rename(&from, &to).with_context(|| format!("renaming {path}"))?;
    let meta = meta_of(&from);
    if meta.exists() {
        std::fs::rename(&meta, meta_of(&to)).with_context(|| format!("renaming {path}'s .meta"))?;
    }
    if path.ends_with(".cs") && name.ends_with(".cs") {
        rename_class(&to, stem(path), stem(name))?;
    }
    Ok(renamed)
}

/// Moves a file or folder, with its `.meta`, into `folder` (from the
/// project's folder, `""` for the project's own), and returns its new path.
/// Its GUID goes with it, so whatever refers to it still does.
pub fn move_into(root: &Path, path: &str, folder: &str) -> Result<String> {
    ensure!(!path.is_empty(), "the project's own folder cannot move");
    let name = path.rsplit('/').next().unwrap_or(path);
    let moved = join(folder, name);
    if moved == path {
        return Ok(moved);
    }
    ensure!(folder != path && !folder.starts_with(&format!("{path}/")), "a folder cannot go inside itself");
    let to = root.join(&moved);
    let shown = if folder.is_empty() { "the project's folder" } else { folder };
    ensure!(!to.exists(), "{name} is already in {shown}");
    let from = root.join(path);
    std::fs::rename(&from, &to).with_context(|| format!("moving {path}"))?;
    let meta = meta_of(&from);
    if meta.exists() {
        std::fs::rename(&meta, meta_of(&to)).with_context(|| format!("moving {path}'s .meta"))?;
    }
    Ok(moved)
}

/// Copies a file beside itself, without its `.meta`, and returns the copy's
/// path. Folders are not copied.
pub fn duplicate(root: &Path, path: &str) -> Result<String> {
    let from = root.join(path);
    ensure!(from.is_file(), "only files can be duplicated");
    let name = path.rsplit('/').next().unwrap_or(path);
    let (base, extension) = split_extension(name);
    let copy = join(parent(path), &unique(&root.join(parent(path)), base, extension));
    std::fs::copy(&from, root.join(&copy)).with_context(|| format!("copying {path}"))?;
    if path.ends_with(".cs") {
        rename_class(&root.join(&copy), base, stem(&copy))?;
    }
    Ok(copy)
}

/// Copies files and folders from outside the project into `folder` (from
/// the project's folder), as dropped on the Project panel, and returns their
/// project paths. A name already taken is numbered, and a script then
/// renamed to match. A `.meta` dropped with its file follows it, keeping its
/// GUID and import settings; any other `.meta` is left out. A file already
/// in `folder` stays as it is.
pub fn bring_in(root: &Path, folder: &str, files: &[PathBuf]) -> Result<Vec<String>> {
    let into = root.join(folder);
    let is_meta = |file: &&PathBuf| file.extension().is_some_and(|e| e == "meta");
    let (metas, files): (Vec<&PathBuf>, Vec<&PathBuf>) = files.iter().partition(is_meta);
    let mut brought = Vec::new();
    // Each file's name, and the name it has in the project.
    let mut named = Vec::new();
    for file in files {
        let name = file.file_name().context("a file has no name")?.to_string_lossy().into_owned();
        if file.parent() == Some(into.as_path()) {
            brought.push(join(folder, &name));
            continue;
        }
        let (base, extension) = if file.is_dir() { (name.as_str(), "") } else { split_extension(&name) };
        let free = unique(&into, base, extension);
        copy_all(file, &into.join(&free)).with_context(|| format!("copying {}", file.display()))?;
        if free != name && name.ends_with(".cs") {
            rename_class(&into.join(&free), base, stem(&free))?;
        }
        brought.push(join(folder, &free));
        named.push((name, free));
    }
    for meta in metas {
        let name = meta.file_name().unwrap_or_default().to_string_lossy();
        let of = name.trim_end_matches(".meta");
        if let Some((_, free)) = named.iter().find(|(name, _)| name == of) {
            std::fs::copy(meta, into.join(format!("{free}.meta"))).with_context(|| format!("copying {name}"))?;
        }
    }
    Ok(brought)
}

/// Copies a file, or a folder with everything in it.
fn copy_all(from: &Path, to: &Path) -> Result<()> {
    if !from.is_dir() {
        std::fs::copy(from, to)?;
        return Ok(());
    }
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        copy_all(&entry.path(), &to.join(entry.file_name()))?;
    }
    Ok(())
}

/// Moves a file or folder at `path` in `root`, and its `.meta`, into the
/// `.voxear/Trash` of the project at `project`, and returns where it went.
/// `root` is the project's folder, or a folder package's, which so never
/// gets a `.voxear` of its own.
pub fn trash(root: &Path, path: &str, project: &Path) -> Result<PathBuf> {
    let from = root.join(path);
    ensure!(from.exists(), "{path} is not there");
    let bin = project.join(".voxear").join("Trash");
    std::fs::create_dir_all(&bin)?;
    let name = path.rsplit('/').next().unwrap_or(path);
    let to = bin.join(unique(&bin, name, ""));
    std::fs::rename(&from, &to).with_context(|| format!("moving {path} to the trash"))?;
    let meta = meta_of(&from);
    if meta.exists() {
        std::fs::rename(&meta, meta_of(&to))?;
    }
    Ok(to)
}

/// A path for a new file in `folder` (from the project's folder), named
/// `stem` with `extension`, numbered if that is taken.
pub fn free_path(root: &Path, folder: &str, stem: &str, extension: &str) -> String {
    join(folder, &unique(&root.join(folder), stem, extension))
}

/// `stem` and `extension`, with a number before the extension when the name
/// is taken: "New Scene.scene", then "New Scene 1.scene".
pub(crate) fn unique(folder: &Path, stem: &str, extension: &str) -> String {
    let free = |name: &str| !folder.join(name).exists();
    let first = format!("{stem}{extension}");
    if free(&first) {
        return first;
    }
    let spaced = !stem.ends_with(|c: char| c.is_alphanumeric()) || !extension.eq(".cs");
    (1..)
        .map(|n| if spaced { format!("{stem} {n}{extension}") } else { format!("{stem}{n}{extension}") })
        .find(|name| free(name))
        .expect("some number is free")
}

/// A new behaviour, named for its file, in today's C#: a namespace for the
/// whole file, on one line.
fn script(namespace: &str, class: &str) -> String {
    format!(
        "using Voxear;\n\nnamespace {namespace};\n\npublic sealed class {class} : Behaviour\n{{\n\tprotected override void \
         Start()\n\t{{\n\t}}\n\n\tprotected override void Update()\n\t{{\n\t}}\n}}\n"
    )
}

/// Renames the class a script declares, which must match its file.
fn rename_class(file: &Path, from: &str, to: &str) -> Result<()> {
    let valid = to.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && to.chars().all(|c| c.is_alphanumeric() || c == '_');
    if !valid || from == to {
        return Ok(());
    }
    let text = std::fs::read_to_string(file)?;
    let declared = format!("class {from}");
    if !text.contains(&declared) {
        return Ok(());
    }
    std::fs::write(file, text.replacen(&declared, &format!("class {to}"), 1))?;
    Ok(())
}

fn meta_of(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".meta");
    path.with_file_name(name)
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(folder, _)| folder)
}

fn join(folder: &str, name: &str) -> String {
    if folder.is_empty() { name.to_string() } else { format!("{folder}/{name}") }
}

/// A file's name before its extension, and its extension with the dot.
fn split_extension(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(dot) => (&name[..dot], &name[dot..]),
        None => (name, ""),
    }
}

fn stem(name: &str) -> &str {
    let name = name.rsplit('/').next().unwrap_or(name);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_lighting_settings_start_from_golden_hour_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = create(dir.path(), "", NewFile::Lighting, "Game").unwrap();
        assert_eq!(path, "New Lighting.lighting");
        let text = std::fs::read_to_string(dir.path().join(&path)).unwrap();
        let file: voxear_engine::lighting::LightingFile = toml::from_str(&text).unwrap();
        assert_eq!(file.look.as_deref(), Some("golden_hour"));
        file.resolve().unwrap();
    }

    #[test]
    fn a_new_ui_shader_draws_ui_with_a_colour_and_a_speed() {
        let dir = tempfile::tempdir().unwrap();
        let path = create_shader(dir.path(), "", ShaderStart::Ui, "Game").unwrap();
        assert_eq!(path, "New UI Shader.shader");
        let info = voxear_engine::shaders::read(&std::fs::read_to_string(dir.path().join(&path)).unwrap()).unwrap();
        assert!(info.ui && !info.surface, "a UI shader, which materials cannot pick");
        let names: Vec<&str> = info.properties.iter().map(|property| property.name.as_str()).collect();
        assert_eq!(names, ["glow", "speed"]);
    }

    #[test]
    fn files_are_made_renamed_copied_and_trashed_with_their_meta() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("Scenes")).unwrap();
        assert_eq!(create(root, "Scenes", NewFile::Scene, "Game").unwrap(), "Scenes/New Scene.scene");
        assert_eq!(create(root, "Scenes", NewFile::Scene, "Game").unwrap(), "Scenes/New Scene 1.scene", "a free name");
        std::fs::write(root.join("Scenes/New Scene.scene.meta"), "guid = \"x\"\n").unwrap();

        let renamed = rename(root, "Scenes/New Scene.scene", "Main").unwrap();
        assert_eq!(renamed, "Scenes/Main.scene", "a file keeps its extension when the name leaves it out");
        assert_eq!(rename(root, "Scenes/Main.scene", "Main.scene").unwrap(), "Scenes/Main.scene", "or has it");
        assert!(root.join("Scenes/Main.scene.meta").is_file(), "the .meta goes along, keeping the GUID");

        let script = create(root, "", NewFile::Script, "Game").unwrap();
        assert_eq!(script, "NewBehaviour.cs");
        rename(root, &script, "Mover.cs").unwrap();
        assert!(std::fs::read_to_string(root.join("Mover.cs")).unwrap().contains("class Mover : Behaviour"));
        let copy = duplicate(root, "Mover.cs").unwrap();
        assert_eq!(copy, "Mover1.cs");
        assert!(std::fs::read_to_string(root.join(&copy)).unwrap().contains("class Mover1"));

        let gone = trash(root, "Scenes/Main.scene", root).unwrap();
        assert!(!root.join("Scenes/Main.scene").exists() && gone.is_file() && meta_of(&gone).is_file());
    }

    #[test]
    fn files_dropped_from_outside_are_copied_in_with_their_meta() {
        let (outside, project) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let (from, root) = (outside.path(), project.path());
        std::fs::create_dir_all(root.join("Models")).unwrap();
        std::fs::write(root.join("Models/Ship.glb"), "old").unwrap();
        for (name, text) in [("Ship.glb", "new"), ("Ship.glb.meta", "guid = \"x\"\n"), ("Lone.png.meta", "")] {
            std::fs::write(from.join(name), text).unwrap();
        }
        std::fs::create_dir_all(from.join("Rocks/Small")).unwrap();
        std::fs::write(from.join("Rocks/Small/Pebble.glb"), "").unwrap();
        let dropped = ["Ship.glb.meta", "Ship.glb", "Rocks", "Lone.png.meta"].map(|name| from.join(name));

        let brought = bring_in(root, "Models", &dropped).unwrap();
        assert_eq!(brought, ["Models/Ship 1.glb", "Models/Rocks"], "a taken name is numbered");
        assert_eq!(std::fs::read_to_string(root.join("Models/Ship.glb")).unwrap(), "old", "nothing is replaced");
        assert!(root.join("Models/Ship 1.glb.meta").is_file(), "the .meta follows its file");
        assert!(root.join("Models/Rocks/Small/Pebble.glb").is_file(), "a folder comes with what is in it");
        assert!(!root.join("Models/Lone.png.meta").exists(), "a .meta without its file is left out");
    }

    #[test]
    fn a_folder_packages_files_change_in_its_folder_and_a_cached_ones_never() {
        let (project, packages) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let (mine, orbit) = (packages.path().join("mine"), packages.path().join("orbit"));
        std::fs::create_dir_all(mine.join("Rocks")).unwrap();
        std::fs::write(mine.join("Rocks/Rock.png"), "").unwrap();
        let package = |name: &str, root: &Path, access| voxear_assets::PackageMount {
            name: name.into(),
            title: name.into(),
            root: root.to_path_buf(),
            access,
        };
        let mounts = Mounts::new(
            project.path(),
            vec![package("my-tools", &mine, Access::Editable), package("orbit-tools", &orbit, Access::ReadOnly)],
        );

        let at = item_place(&mounts, "Packages/my-tools/Rocks/Rock.png").unwrap();
        assert_eq!((at.root.as_path(), at.path.as_str()), (mine.as_path(), "Rocks/Rock.png"));
        let renamed = rename(&at.root, &at.path, "Boulder").map(|path| at.back(&path)).unwrap();
        assert_eq!(renamed, "Packages/my-tools/Rocks/Boulder.png", "in the package's own folder");
        assert!(mine.join("Rocks/Boulder.png").is_file());

        assert!(place(&mounts, "Packages/orbit-tools/Prefabs").is_err(), "a cached package is read-only");
        assert!(item_place(&mounts, "Packages/my-tools").is_err(), "a package itself is the Package Manager's");
        assert!(place(&mounts, "Packages").is_err(), "the Packages section holds only packages");
        assert!(move_place(&mounts, "Packages/my-tools/Rocks/Boulder.png", "Scenes").is_err(), "nor out of it");
        assert_eq!(place(&mounts, "Scenes").unwrap().root, project.path());
    }
}
