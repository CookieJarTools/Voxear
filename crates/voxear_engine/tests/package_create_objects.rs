//! A folder package adds a prefab of its own to Create Object: its script's
//! `[CreateObject]` on a static `Ref<Prefab>` is described with the
//! package's assembly, and the prefab's path, written from the package's
//! folder, is found in the package's assets. A game script's is from the
//! project's. A file of its own, so it runs in a process of its own, since
//! .NET starts only once per process. Skips itself without .NET.

use std::path::Path;

use voxear_engine::{Content, Project, new_project, script_types};

const LANTERN_OBJECTS: &str = r#"using Voxear;

namespace LanternKit;

internal static class LanternObjects
{
	[CreateObject("Lights/Lantern")]
	private static readonly Ref<Prefab> Lantern = new("Prefabs/Lantern.prefab");
}
"#;

const CRATE_OBJECTS: &str = r#"using Voxear;

namespace Range;

internal static class CrateObjects
{
	[CreateObject("Props/Crate")]
	private static readonly Ref<Prefab> Crate = new("Prefabs/Crate.prefab");
}
"#;

const LANTERN: &str = "format = 1\n[[obj]]\nid = 1\nname = \"Lantern\"\n[[obj]]\nid = 2\nname = \"Glow\"\nparent = 1\n\
                       light = { type = \"point\" }\n";
const CRATE: &str = "format = 1\n[[obj]]\nid = 1\nname = \"Crate\"\n";

fn write(folder: &Path, file: &str, text: &str) {
    let path = folder.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[test]
fn a_package_adds_its_own_prefab_to_create_object() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let kit = dir.path().join("lantern-kit");
    write(&kit, "package.toml", "[package]\nname = \"lantern-kit\"\nversion = \"1.0.0\"\nengine = \">=0.1\"\n");
    write(&kit, "Scripts/LanternObjects.cs", LANTERN_OBJECTS);
    write(&kit, "Prefabs/Lantern.prefab", LANTERN);
    let folder = dir.path().join("Range");
    let request = new_project::NewProject {
        folder: folder.clone(),
        name: "Range".into(),
        template: "empty".into(),
        setup: Some(vec!["none".into()]),
        ide: None,
        git: false,
        modding: false,
        steam_app_id: None,
    };
    new_project::create(&request, &mut |_| {}).unwrap();
    write(&folder, "packages.toml", "[packages]\nlantern-kit = { path = \"../lantern-kit\" }\n");
    write(&folder, "Scripts/CrateObjects.cs", CRATE_OBJECTS);
    write(&folder, "Prefabs/Crate.prefab", CRATE);

    let types = script_types::describe(&Project::open(&folder).unwrap()).unwrap();
    let lantern = types.create_objects.iter().find(|made| made.menu == "Lights/Lantern").expect("the package's");
    assert_eq!((lantern.prefab.as_str(), lantern.assembly.as_str()), ("Prefabs/Lantern.prefab", "LanternKit"));
    assert_eq!(lantern.place(), ("Lights", "Lantern"));
    let crate_ = types.create_objects.iter().find(|made| made.menu == "Props/Crate").expect("the game's own");

    // The package's path is from its folder, as the content knows its assets; the game's from the project's.
    let content = Content::open(&folder).unwrap();
    assert_eq!(lantern.prefab_path(content.db()), "Packages/lantern-kit/Prefabs/Lantern.prefab");
    assert!(content.db().guid_of(&lantern.prefab_path(content.db())).is_some(), "a prefab the content has");
    assert_eq!(crate_.prefab_path(content.db()), "Prefabs/Crate.prefab");
}
