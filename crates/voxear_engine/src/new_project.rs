//! New projects, as `vox new` makes them for the launcher, which never
//! writes project files itself: it asks what this engine offers
//! ([`describe`]), builds its form from the answer, and has `vox new` make
//! the project ([`create`]), so the launcher, `vox` and the editor make
//! identical projects. See "New-project flow" in the launcher's design.
//!
//! A project gets `voxear.toml` pinned to this engine's version, a level
//! and its scene, a scripts project with a solution beside it, and the
//! `.meta` files that give its assets their GUIDs. Templates offer only what
//! the engine can make today: Empty and 3D. The first-person, third-person,
//! top-down and 2D templates wait for an input API and 2D rendering.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};

use crate::project::{PROJECT_FILE, engine_version};

/// The format of [`describe`]'s answer.
pub const FORMAT: u32 = 1;

/// The longest name a project may have, which is also its folder's name.
const NAME_LIMIT: usize = 64;

/// Names Windows keeps for devices, which no file or folder may take.
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2",
    "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// A template: its ID, name, what it is, and the setup it turns on.
struct Template {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    setup: &'static [&'static str],
}

const TEMPLATES: [Template; 2] = [
    Template { id: "empty", name: "Empty", description: "Nothing but a camera", setup: &["camera"] },
    Template {
        id: "3d",
        name: "3D",
        description: "A lit 3D scene with a ground and a turning cube",
        setup: &["lighting", "camera", "sample-scene"],
    },
];

/// Setup items: ID, name and what each adds.
const SETUP: [(&str, &str, &str); 3] = [
    ("lighting", "Lighting and sky", "A sun and a daytime sky"),
    ("camera", "Camera", "A camera looking at the middle of the scene"),
    ("sample-scene", "Sample scene", "A ground and a cube that turns, with the script that turns it"),
];

const IDES: [(&str, &str); 3] = [("rider", "Rider"), ("visual-studio", "Visual Studio"), ("vscode", "VS Code")];

/// What a new project is to be, as `vox new`'s flags give it.
#[derive(Clone, Debug)]
pub struct NewProject {
    /// The project folder itself, which must not exist or must be empty.
    pub folder: PathBuf,
    pub name: String,
    pub template: String,
    /// Setup items, or the template's own when `None`.
    pub setup: Option<Vec<String>>,
    pub ide: Option<String>,
    pub git: bool,
    pub modding: bool,
    pub steam_app_id: Option<u64>,
}

/// What this engine's `vox new` offers, for the launcher's form.
pub fn describe() -> Value {
    let templates: Vec<Value> = TEMPLATES
        .iter()
        .map(|t| json!({ "id": t.id, "name": t.name, "description": t.description, "setup": t.setup }))
        .collect();
    let setup: Vec<Value> = SETUP
        .iter()
        .map(|(id, name, description)| json!({ "id": id, "name": name, "description": description }))
        .collect();
    let ides: Vec<Value> = IDES.iter().map(|(id, name)| json!({ "id": id, "name": name })).collect();
    json!({ "format": FORMAT, "templates": templates, "setup": setup, "ides": ides })
}

/// Makes the project, reporting each step to `step` as it starts. Returns
/// the project folder. Nothing is written until the request is checked.
pub fn create(request: &NewProject, step: &mut dyn FnMut(&str)) -> Result<PathBuf> {
    check_name(&request.name)?;
    let template = TEMPLATES
        .iter()
        .find(|t| t.id == request.template)
        .with_context(|| format!("this version of Voxear has no {} template", request.template))?;
    let setup = chosen_setup(template, request.setup.as_deref())?;
    if let Some(ide) = request.ide.as_deref() {
        ensure!(IDES.iter().any(|(id, _)| *id == ide), "this version of Voxear does not set up {ide}");
    }
    let folder = &request.folder;
    ensure!(is_empty_or_missing(folder), "{} is not empty", folder.display());

    step(&format!("Making the {} project {}", template.name, request.name));
    let namespace = namespace(&request.name);
    let has = |item: &str| setup.iter().any(|chosen| chosen == item);
    for sub in ["Levels", "Scenes", "Scripts", ".voxear"] {
        std::fs::create_dir_all(folder.join(sub)).with_context(|| format!("making {}", folder.display()))?;
    }
    write(folder, PROJECT_FILE, &project_file(&request.name, &namespace))?;
    write(folder, "Levels/Main.level", MAIN_LEVEL)?;

    step("Writing the scene");
    write(folder, "Scenes/Main.scene", &scene(&namespace, has("lighting"), has("camera"), has("sample-scene")))?;

    step("Writing the scripts project");
    write(folder, &format!("Scripts/{namespace}.Scripts.csproj"), &scripts_project(&namespace))?;
    write(folder, &format!("{namespace}.slnx"), &solution(&namespace))?;
    write(folder, ".voxear/Engine.props", &crate::scripts::engine_props())?;
    if has("sample-scene") {
        write(folder, "Scripts/Turner.cs", &turner(&namespace))?;
    }

    step("Adding the build profiles");
    crate::build_profile::write_default_profiles(folder)?;

    if let Some(ide) = request.ide.as_deref() {
        set_up_ide(folder, ide, &namespace, step)?;
    }
    if request.modding {
        step("Mods are not in this version of Voxear yet, so modding was left off");
    }
    if request.steam_app_id.is_some() {
        step("Steam is not in this version of Voxear yet, so the Steam App ID was left out");
    }

    step("Giving the assets their IDs");
    voxear_assets::AssetDb::scan(folder).context("writing the assets' .meta files")?;
    if request.git {
        step("Starting a git repository");
        set_up_git(folder)?;
    }
    Ok(folder.clone())
}

/// A name that is also a folder name on Linux and Windows.
pub fn check_name(name: &str) -> Result<()> {
    ensure!(!name.trim().is_empty(), "a project needs a name");
    ensure!(name.chars().count() <= NAME_LIMIT, "a project name can be at most {NAME_LIMIT} characters");
    let bad = name.chars().find(|c| r#"/\:*?"<>|"#.contains(*c) || c.is_control());
    if let Some(bad) = bad {
        bail!(
            "a project name cannot contain {}",
            if bad.is_control() { "control characters".into() } else { format!("{bad}") }
        );
    }
    ensure!(!name.ends_with('.') && !name.ends_with(' '), "a project name cannot end with a full stop or a space");
    let stem = name.split('.').next().unwrap_or(name).trim().to_ascii_uppercase();
    ensure!(!RESERVED.contains(&stem.as_str()), "{name} is a name Windows keeps for itself");
    Ok(())
}

/// The setup chosen, or the template's own; `none` turns everything off.
fn chosen_setup(template: &Template, asked: Option<&[String]>) -> Result<Vec<String>> {
    let Some(asked) = asked else { return Ok(template.setup.iter().map(|s| s.to_string()).collect()) };
    if asked.iter().any(|item| item == "none") {
        return Ok(Vec::new());
    }
    for item in asked {
        ensure!(SETUP.iter().any(|(id, ..)| id == item), "this version of Voxear has no {item} setup");
    }
    Ok(asked.to_vec())
}

fn is_empty_or_missing(folder: &Path) -> bool {
    std::fs::read_dir(folder).map_or(true, |mut entries| entries.next().is_none())
}

/// The project's name as a C# namespace: letters, digits and underscores,
/// in PascalCase where it had spaces, never starting with a digit.
pub fn namespace(name: &str) -> String {
    let mut out = String::new();
    for word in name.split(|c: char| !c.is_alphanumeric() && c != '_').filter(|w| !w.is_empty()) {
        let mut letters = word.chars();
        if let Some(first) = letters.next() {
            out.extend(first.to_uppercase());
            out.extend(letters);
        }
    }
    match out.chars().next() {
        None => "Game".into(),
        Some(first) if first.is_ascii_digit() => format!("_{out}"),
        Some(_) => out,
    }
}

fn write(folder: &Path, path: &str, text: &str) -> Result<()> {
    let file = folder.join(path);
    std::fs::write(&file, text).with_context(|| format!("writing {}", file.display()))
}

fn project_file(name: &str, namespace: &str) -> String {
    let name = name.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        "[project]\nname = \"{name}\"\nscripts = \"Scripts/{namespace}.Scripts.csproj\"\nstart_level = \"Levels/Main.level\"\n\
         # The Voxear version this project uses, which the launcher opens it with.\nengine = \"{}\"\n\n\
         # The engine's optional systems, switched on in Project Settings.\n[modules]\n{}",
        engine_version(),
        crate::modules::Module::ALL.map(|module| format!("{} = false\n", module.key())).concat()
    )
}

const MAIN_LEVEL: &str =
    "# Voxear level: the scene the game starts in.\nformat = 1\n\n[[scene]]\nscene = \"Scenes/Main.scene\"\n";

/// The main scene, with what the setup asks for.
fn scene(namespace: &str, lighting: bool, camera: bool, sample: bool) -> String {
    let mut text = String::from("# Voxear scene\nformat = 1\n");
    if lighting {
        text.push_str(
            "\n[sky]\nzenith = [0.517, 0.701, 0.931]\nhorizon = [0.809, 0.865, 0.936]\nground = [0.506, 0.485, 0.461]\n\
             ambient = 1.0\n",
        );
    }
    let mut objs: Vec<String> = Vec::new();
    if lighting {
        objs.push(
            "name = \"Sun\"\nrotation = [42.0, -35.0, 0.0]\n\
             light = { type = \"directional\", color = [1.0, 0.95, 0.88], intensity = 3.2 }\n"
                .into(),
        );
    }
    if camera {
        objs.push("name = \"Main Camera\"\nposition = [0.0, 2.2, -7.0]\nrotation = [12.0, 0.0, 0.0]\ncamera = { fov = 55.0 }\n".into());
    }
    if sample {
        objs.push(
            "name = \"Ground\"\nscale = [30.0, 1.0, 30.0]\n[obj.renderer]\nmesh = \"builtin:plane\"\n\
             material = { color = [0.42, 0.44, 0.46], roughness = 0.9 }\n"
                .into(),
        );
        objs.push(format!(
            "name = \"Cube\"\nposition = [0.0, 0.75, 0.0]\nscale = [1.5, 1.5, 1.5]\nbehaviours = [\"{namespace}.Turner\"]\n\
             [obj.renderer]\nmesh = \"builtin:cube\"\nmaterial = {{ color = [0.55, 0.36, 0.96], roughness = 0.35 }}\n"
        ));
    }
    for (index, obj) in objs.iter().enumerate() {
        text.push_str(&format!("\n[[obj]]\nid = {}\n{obj}", index + 1));
    }
    text
}

fn scripts_project(namespace: &str) -> String {
    format!(
        r#"<Project Sdk="Microsoft.NET.Sdk">
	<!--
		The engine's settings and Voxear.Core come from the engine building the scripts, which the editor and vox name
		as VoxearHome. An IDE names none, so .voxear/Engine.props, which each build writes, names the engine for it.
	-->
	<Import Project="../.voxear/Engine.props" Condition="'$(VoxearHome)' == '' and Exists('../.voxear/Engine.props')" />
	<Import Project="$(VoxearHome)managed/Voxear.Scripts.props" />
	<PropertyGroup>
		<RootNamespace>{namespace}</RootNamespace>
		<OutputPath>$(MSBuildProjectDirectory)/../.voxear/Scripts/$(Configuration)/</OutputPath>
		<!-- [SerializeField] fields are set by the engine, which the compiler cannot see. -->
		<NoWarn>$(NoWarn);CS0649</NoWarn>
	</PropertyGroup>
</Project>
"#
    )
}

fn solution(namespace: &str) -> String {
    format!("<Solution>\n\t<Project Path=\"Scripts/{namespace}.Scripts.csproj\" />\n</Solution>\n")
}

/// The sample scene's script, which turns the cube.
fn turner(namespace: &str) -> String {
    format!(
        r#"using System.Numerics;
using Voxear;

namespace {namespace};

/// <summary>Turns its Obj around the up axis. Change the speed in the Inspector, or this script while playing.</summary>
public sealed class Turner : Behaviour
{{
	[SerializeField] private float degreesPerSecond = 45f;

	protected override void Update()
	{{
		Obj.Rotation = Quaternion.Normalize(Obj.Rotation * Quaternion.AngleAxis(degreesPerSecond * Time.Delta, Vector3.Up));
	}}
}}
"#
    )
}

/// The C# IDE's own files: the solution suits Rider and Visual Studio as it
/// is, and VS Code is told to use it with C# Dev Kit.
fn set_up_ide(folder: &Path, ide: &str, namespace: &str, step: &mut dyn FnMut(&str)) -> Result<()> {
    let name = IDES.iter().find(|(id, _)| *id == ide).map_or(ide, |(_, name)| name);
    step(&format!("Setting up {name}: open {namespace}.slnx"));
    if ide != "vscode" {
        return Ok(());
    }
    std::fs::create_dir_all(folder.join(".vscode"))?;
    write(folder, ".vscode/extensions.json", "{\n\t\"recommendations\": [\"ms-dotnettools.csdevkit\"]\n}\n")?;
    write(folder, ".vscode/settings.json", &format!("{{\n\t\"dotnet.defaultSolution\": \"{namespace}.slnx\"\n}}\n"))
}

/// A git repository that leaves out what Voxear and .NET build and keep
/// per machine.
fn set_up_git(folder: &Path) -> Result<()> {
    write(
        folder,
        ".gitignore",
        "# Voxear's build output and caches, and this machine's editor settings.\n.voxear/\n# C# build output.\nbin/\nobj/\n",
    )?;
    write(folder, ".gitattributes", "* text=auto\n*.glb binary\n*.png binary\n*.jpg binary\n*.ktx2 binary\n")?;
    let status = Command::new("git").arg("init").arg("--quiet").arg(folder).status().context("running git")?;
    ensure!(status.success(), "git init failed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_fit_every_platform_and_make_namespaces() {
        assert!(check_name("Orbit").is_ok());
        assert!(check_name("My Game 2").is_ok());
        for bad in ["", "a/b", "what?", "trailing.", "con", "COM1.txt", &"x".repeat(65)] {
            assert!(check_name(bad).is_err(), "{bad:?}");
        }
        assert_eq!(namespace("Orbit"), "Orbit");
        assert_eq!(namespace("my space game"), "MySpaceGame");
        assert_eq!(namespace("2048"), "_2048");
        assert_eq!(namespace("---"), "Game");
    }

    #[test]
    fn a_new_3d_project_opens_with_its_level_scene_and_scripts() {
        let parent = tempfile::tempdir().unwrap();
        let folder = parent.path().join("Orbit Run");
        let request = NewProject {
            folder: folder.clone(),
            name: "Orbit Run".into(),
            template: "3d".into(),
            setup: None,
            ide: Some("vscode".into()),
            git: false,
            modding: true,
            steam_app_id: None,
        };
        let mut steps = Vec::new();
        create(&request, &mut |step| steps.push(step.to_string())).unwrap();
        assert!(steps.iter().any(|s| s.starts_with("Mods are not")), "{steps:?}");

        let project = crate::Project::open(&folder).unwrap();
        assert_eq!(project.file.project.engine.as_deref(), Some(engine_version()));
        assert_eq!(project.scripts_project(), Some(folder.join("Scripts/OrbitRun.Scripts.csproj")));
        let scene = crate::scene_file::read(&folder.join("Scenes/Main.scene")).unwrap();
        let names: Vec<&str> = scene.objs.iter().map(|o| o.name.as_str()).collect();
        assert_eq!(names, ["Sun", "Main Camera", "Ground", "Cube"]);
        assert!(folder.join("Scenes/Main.scene.meta").is_file(), "assets have their GUIDs");
        assert_eq!(
            crate::build_profile::default_profile(&folder).as_deref(),
            Some(if cfg!(windows) { "Windows Release" } else { "Linux Release" })
        );
        assert!(folder.join(".vscode/settings.json").is_file());
        assert!(create(&request, &mut |_| {}).is_err(), "a folder with something in it is refused");
    }

    #[test]
    fn the_description_lists_what_create_accepts() {
        let offer = describe();
        assert_eq!(offer["format"], FORMAT);
        let ids: Vec<&str> = offer["templates"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["empty", "3d"]);
    }
}
