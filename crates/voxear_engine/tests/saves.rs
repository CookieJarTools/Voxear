//! Saves from C#, on a copy of Hello with a script of its own: values with
//! Objs, behaviours and prefabs in them written to slots and read back, a
//! behaviour's fields put back into another, slots listed, copied, renamed
//! and deleted, a write while the game carries on, and saves from outside
//! the game that are damaged, from a newer version, or from an older one to
//! migrate. Each lands in a save folder of the test's own, as a file the
//! engine wrote whole. A file of its own, so it runs in a process of its own,
//! since .NET starts only once per process. Skips itself without .NET.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_scripting::{SaveFolder, SavePart, SaveSettings};

const SCRIPT: &str = r#"using System.Collections.Generic;
using System.Numerics;
using Voxear;

namespace Hello;

[SaveVersion(2)]
public sealed class Progress
{
	public int Level = 1;
	public Obj Checkpoint;
	public Spinner? Favourite;
	public Ref<Prefab> Pet;
	public Prefab Lamp;
	public Vector3 Position;
	public List<string> Found = [];

	// Version 1 called the level its stage.
	[MigrateFrom(1)]
	private static void RenameStage(SaveNode data) => data.Rename("Stage", "Level");
}

public sealed class SaveCheck : SystemBase
{
	private int frame;

	protected override void Update()
	{
		switch (++frame)
		{
			case 2:
				WriteAndRead();
				break;
			case 5:
				ReadWhatCameFromOutside();
				break;
		}
	}

	private static void WriteAndRead()
	{
		var spinnerA = Obj.Find("Spinner A");
		var progress = new Progress
		{
			Level = 3,
			Checkpoint = spinnerA,
			Favourite = Obj.Find("Spinner B").GetBehaviour<Spinner>(),
			Pet = new Ref<Prefab>("Prefabs/Firefly.prefab"),
			Lamp = Prefab.Load("Prefabs/Lamp.prefab"),
			Position = new Vector3(1, 2, 3),
			Found = ["key", "map"],
		};
		Save.Write("slot1", progress, "Chapter 3");
		var back = Save.Read<Progress>("slot1");
		Log.Info($"read level {back.Level}, checkpoint {back.Checkpoint.Name}, favourite {back.Favourite?.Obj.Name}, pet {back.Pet.Path}, lamp {back.Lamp == progress.Lamp}, found {string.Join("+", back.Found)}");

		var made = progress.Lamp.Instantiate();
		Save.Write("made", new Progress { Checkpoint = made });

		var spinner = spinnerA.GetBehaviour<Spinner>()!;
		Save.Write("spinner", spinner);
		var follower = Obj.Find("Spinner B").GetBehaviour<Spinner>()!;
		Save.ReadInto("spinner", follower);
		Log.Info($"follower turns at {follower.DegreesPerSecond}");

		Save.Copy("slot1", "backup");
		Save.Rename("backup", "Backup 2");
		foreach (var slot in Save.Slots)
		{
			Log.Info($"slot {slot.Name}: {slot.Summary ?? "-"}, version {slot.Version}, {slot.Format}");
		}
		Log.Info($"deleted {Save.Delete("Backup 2")}, exists {Save.Exists("Backup 2")}");

		try
		{
			Save.Read<Progress>("nothing");
		}
		catch (SaveException missing)
		{
			Log.Info($"missing: {missing.Error}");
		}
		try
		{
			Save.Write("a/b", 1);
		}
		catch (SaveException bad)
		{
			Log.Info($"bad name: {bad.Error}");
		}

		_ = WriteWhileGoingOn(progress);
	}

	private static async System.Threading.Tasks.Task WriteWhileGoingOn(Progress progress)
	{
		var write = Save.WriteAsync("async", progress);
		progress.Level = 99;
		await write;
		Log.Info($"written while going on, level {Save.Read<Progress>("async").Level}");
	}

	private static void ReadWhatCameFromOutside()
	{
		Log.Info($"writing {Save.Format.Name}");
		Save.Write("binary", new Progress { Level = 4 });
		Log.Info($"old save at level {Save.Read<Progress>("old").Level}");
		foreach (var slot in new[] { "slot1", "future" })
		{
			try
			{
				Save.Read<Progress>(slot);
			}
			catch (SaveException refused)
			{
				Log.Info($"{slot}: {refused.Error}: {refused.Message}");
			}
		}
	}
}
"#;

/// A copy of Hello, inside the repository, where its scripts find the
/// engine to build against.
fn hello_copy() -> tempfile::TempDir {
    fn copy(from: &std::path::Path, to: &std::path::Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "bin" || name == "obj" {
                continue;
            }
            if entry.path().is_dir() {
                copy(&entry.path(), &to.join(&name));
            } else {
                std::fs::copy(entry.path(), to.join(&name)).unwrap();
            }
        }
    }
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    copy(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/Hello"), dir.path());
    dir
}

fn guid_of(project: &std::path::Path, asset: &str) -> String {
    let meta = std::fs::read_to_string(project.join(format!("{asset}.meta"))).unwrap();
    meta.split('"').nth(1).unwrap().to_string()
}

#[test]
fn scripts_save_and_read_back_through_the_engines_files() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/SaveCheck.cs"), SCRIPT).unwrap();
    let saves = dir.path().join("Saves");
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    let mut runtime = scripts::start(&project).unwrap().expect("Hello has scripts");
    // Never the player's own save folder.
    runtime.set_saves(SaveSettings { folder: Some(saves.clone()), readable: true });
    engine.set_scripts(runtime);
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    for _ in 0..3 {
        game.tick(1.0 / 60.0, None);
    }
    let said = |log: &LogCapture| -> Vec<String> {
        let lines = log.lock().unwrap();
        lines.lines.iter().filter(|l| l.target == "script").map(|l| l.message.clone()).collect()
    };
    let lines = said(&log);
    let firefly = guid_of(dir.path(), "Prefabs/Firefly.prefab");
    let read =
        format!("read level 3, checkpoint Spinner A, favourite Spinner B, pet {firefly}, lamp True, found key+map");
    assert!(lines.contains(&read), "{lines:#?}");
    assert!(
        lines.contains(&"follower turns at 90".to_string()),
        "a behaviour's fields put back into another: {lines:#?}"
    );
    assert!(lines.iter().any(|l| l.contains("was made while the game ran")), "a runtime Obj is named: {lines:#?}");
    for slot in ["slot Backup 2: Chapter 3, version 2, json", "slot spinner: -, version 1, json"] {
        assert!(lines.iter().any(|l| l == slot), "{slot}: {lines:#?}");
    }
    assert!(lines.contains(&"deleted True, exists False".to_string()), "{lines:#?}");
    assert!(
        lines.contains(&"missing: Missing".to_string()) && lines.contains(&"bad name: BadName".to_string()),
        "{lines:#?}"
    );
    assert!(lines.contains(&"written while going on, level 3".to_string()), "taken apart when asked: {lines:#?}");

    // The files are the engine's: whole, with nothing left half-written.
    let folder = SaveFolder::new(&saves);
    let mut files: Vec<String> =
        std::fs::read_dir(&saves).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    files.sort();
    assert_eq!(files, ["async.save", "made.save", "slot1.save", "spinner.save"]);
    let info = String::from_utf8(folder.read("slot1", SavePart::Info).unwrap()).unwrap();
    assert!(info.contains(r#""type":"Hello.Progress""#) && info.contains(r#""summary":"Chapter 3""#), "{info}");
    let data = String::from_utf8(folder.read("slot1", SavePart::Data).unwrap()).unwrap();
    let day = guid_of(dir.path(), "Scenes/Day.scene");
    assert!(data.contains(&format!("\"Checkpoint\": \"{day}:2\"")), "an Obj by its scene and place: {data}");
    assert!(data.contains(&format!("\"Favourite\": \"{day}:3\"")), "a behaviour by its Obj: {data}");
    assert!(data.contains("\"$version\": 2") && data.contains(&format!("\"Pet\": \"{firefly}\"")), "{data}");
    let made = String::from_utf8(folder.read("made", SavePart::Data).unwrap()).unwrap();
    assert!(made.contains("\"Checkpoint\": null"), "{made}");

    // Saves from outside: a damaged one, one from a newer version, and one from an older version.
    game.engine.scripts_mut().unwrap().set_saves(SaveSettings { folder: Some(saves.clone()), readable: false });
    let path = saves.join("slot1.save");
    let mut bytes = std::fs::read(&path).unwrap();
    let last = bytes.len() - 2;
    bytes[last] ^= 0x20;
    std::fs::write(&path, bytes).unwrap();
    folder.write("future", br#"{"format":"json","version":3}"#, &[], br#"{"$version": 3, "Level": 1}"#).unwrap();
    folder.write("old", br#"{"format":"json"}"#, &[], br#"{"Stage": 7}"#).unwrap();
    for _ in 0..2 {
        game.tick(1.0 / 60.0, None);
    }
    let lines = said(&log);
    assert!(lines.contains(&"writing binary".to_string()), "builds write binary: {lines:#?}");
    assert!(folder.read("binary", SavePart::Data).unwrap().starts_with(b"VSB"));
    assert!(lines.contains(&"old save at level 7".to_string()), "migrated: {lines:#?}");
    let damaged = lines.iter().find(|l| l.starts_with("slot1: ")).expect("slot1 read");
    assert!(damaged.starts_with("slot1: Damaged: ") && damaged.contains("changed after it was written"), "{damaged}");
    let newer = lines.iter().find(|l| l.starts_with("future: ")).expect("future read");
    assert!(newer.starts_with("future: NewerVersion: ") && newer.contains("up to version 2"), "{newer}");
}
