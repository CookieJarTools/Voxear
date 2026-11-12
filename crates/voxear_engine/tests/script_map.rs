//! Maps from C#: a behaviour gives its Obj a Map and sets it, marks an Obj
//! and a place, names what every map follows and its title, and puts a
//! minimap of its own in a UI made in code. On a copy of Hello. A file of
//! its own, so it runs in a process of its own, since .NET starts only once
//! per process. Skips itself without .NET.

use voxear_core::{logging::LogCapture, math::Vec3};
use voxear_ecs::Entity;
use voxear_engine::{
    Engine, Game, Project,
    map::{Map, MapCorner, MapIcon, MapMarker, MapScript, MarkerShows},
    scripts,
};
use voxear_scene::Name;
use voxear_ui::MapViewKind;

const DT: f32 = 1.0 / 60.0;

const SCRIPT: &str = r##"using System.Numerics;
using Voxear;

namespace Hello;

public sealed class MapScript : Behaviour
{
	private MapPin? camp;
	private int frames;

	protected override void Start()
	{
		var map = Obj.AddMap();
		map.Corner = MapCorner.BottomLeft;
		map.Zoom = 8;
		map.Round = true;
		Map.Follow = Obj;
		Map.Title = "Oakrest Vale";
		Map.Time = "14:12";
		Map.PlayerName = "Pixie";
		Map.AddMarker(Obj, MapIcon.Player, "Me", Color.Green);
		camp = Map.AddMarker(new Vector3(12f, 3f, -4f), MapIcon.Camp, "Old camp");
		camp.StaysAtEdge = true;
		var gone = Map.AddMarker(Vector3.Zero, MapIcon.Skull);
		gone.Remove();

		// This player's map knows what it is shown; another's, what it is sent.
		map.Shows = MapShows.GameDecides;
		var threat = MapLayer.Create(Color.Clear, new Color(0.9f, 0.1f, 0.1f, 0.7f));
		var mine = MapKnowledge.Shown;
		mine.Reveal(new Vector3(100f, 0f, 100f), 40f);
		var before = mine.Stamp;
		mine.Paint(threat, new Vector3(600f, 0f, 0f), 50f, 0.5f, 12);
		var theirs = new MapKnowledge();
		theirs.Merge(mine.ChangesSince(before));
		var saved = mine.ToBytes();
		var loaded = new MapKnowledge();
		loaded.Load(saved);
		Log.Info($"known {mine.Knows(new Vector3(100f, 0f, 100f))} {theirs.Knows(new Vector3(100f, 0f, 100f))} {theirs.News(threat, new Vector3(600f, 0f, 0f))} {theirs.NewsHeard(threat, new Vector3(600f, 0f, 0f))} {loaded.Knows(new Vector3(100f, 0f, 100f))} {saved.Length < 600}");

		var hud = UiDocument.Create();
		hud.Root.Add(new Minimap { Zoom = 16, TurnsWithView = false, Tilt = 90f });
		Log.Info($"map {map.IsValid} {map.Corner} {map.Zoom} {map.Round} follows {Map.Follow == Obj} marker {Obj.MapMarker.Label} {Obj.MapMarker.Icon}");
	}

	protected override void Update()
	{
		frames++;
		if (frames == 3)
		{
			camp!.Position = new Vector3(50f, 3f, -4f);
			camp.Label = "New camp";
			Obj.Map.Open();
			// Its own map as an element: every setting a MapView has, and the camera it is seen through.
			var view = Obj.Map.MapView!;
			view.Fill = 1.4f;
			view.ShowCompass = false;
			view.MaxZoom = 16;
			view.Camera.Turn = 90f;
			view.Camera.Zoom = 6f;
			view.Camera.LookAt(new Vector3(200f, 0f, 50f));
			var map = Obj.Map;
			map.PointerControl = false;
			map.MapSize = 1.3f;
			Log.Info($"view {view.Fill} {view.ShowCompass} {Obj.Map.MinimapView != null} {view.Camera.PlaceAt(new Vector2(10f, 10f)) == null}");
		}
	}
}
"##;

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

fn frame(game: &mut Game) {
    game.tick(DT, None);
    game.ui.frame(DT, (800, 600), 1.0);
}

#[test]
fn a_script_sets_up_a_map_marks_an_obj_and_a_place_and_makes_a_minimap_of_its_own() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/MapScript.cs"), SCRIPT).unwrap();
    let scene = dir.path().join("Scenes/Day.scene");
    let before = std::fs::read_to_string(&scene).unwrap();
    let added =
        "\n[[obj]]\nid = 900\nname = \"Mapper\"\nposition = [3.0, 1.0, 2.0]\nbehaviours = [\"Hello.MapScript\"]\n";
    std::fs::write(&scene, format!("{before}{added}")).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    let obj: Entity =
        game.engine.world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == "Mapper").unwrap().0;
    frame(&mut game);
    frame(&mut game);

    let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect();
    let said = "map True BottomLeft 8 True follows True marker Me Player";
    assert!(lines.iter().any(|line| line == said), "{lines:?}");
    // Its own map knows what it revealed; the other's only the news it was sent; one loaded, what was saved.
    assert!(
        lines.iter().any(|line| line.starts_with("known True False 0.5") && line.ends_with("12 True True")),
        "{lines:?}"
    );
    let known = game.engine.world.resource::<voxear_engine::map::MapKnown>().unwrap();
    assert_eq!((known.shows, known.all.len(), known.layers.len()), (voxear_engine::map::MapShows::GameDecides, 3, 1));
    // The Map on its Obj is as the script set it, and shut; its marker marks it.
    let map = game.engine.world.get::<Map>(obj).unwrap().clone();
    assert_eq!((map.corner, map.zoom, map.round, map.open), (MapCorner::BottomLeft, 8, true, false));
    let marker = game.engine.world.get::<MapMarker>(obj).unwrap().clone();
    assert_eq!((marker.icon, marker.label.as_str(), marker.color), (MapIcon::Player, "Me", [0.0, 1.0, 0.0]));
    // Every map follows the Obj and takes its title; the pin kept is at its place, the one removed is gone.
    let script = game.engine.world.resource::<MapScript>().unwrap().clone();
    assert_eq!((script.follow, script.title.as_deref()), (Some(obj), Some("Oakrest Vale")));
    assert_eq!((script.time.as_deref(), script.player.as_deref()), (Some("14:12"), Some("Pixie")));
    assert_eq!(script.placed.len(), 1);
    let camp = script.placed.values().next().unwrap();
    assert_eq!(
        (camp.at, camp.marker.icon, camp.marker.label.as_str()),
        (Vec3::new(12.0, 3.0, -4.0), MapIcon::Camp, "Old camp")
    );
    assert!(camp.marker.stays_at_edge && camp.marker.shows == MarkerShows::Both);

    // On the screen: the Map's minimap and map, and the script's own minimap, set as it said.
    let ui = game.ui.ui();
    let views = ui.map_views();
    assert_eq!(views.len(), 3);
    let own = views.iter().filter_map(|view| ui.map_view_settings(*view)).find(|settings| settings.zoom == 16);
    let own = own.expect("the script's minimap");
    assert_eq!((own.kind, own.turns, own.tilt), (MapViewKind::Minimap, false, 90.0));
    let rounded = views.iter().filter_map(|view| ui.map_view_settings(*view)).filter(|settings| settings.round).count();
    assert_eq!(rounded, 1, "the Map's own minimap is round, as the script set it");

    // Later frames: the pin moved and named anew, and the map opened.
    frame(&mut game);
    frame(&mut game);
    let script = game.engine.world.resource::<MapScript>().unwrap();
    let camp = script.placed.values().next().unwrap();
    assert_eq!((camp.at.x, camp.marker.label.as_str()), (50.0, "New camp"));
    assert!(game.engine.world.get::<Map>(obj).unwrap().open);
    // Its map's settings, set as an element's are, and how it is to be seen, waiting for it to be shown: Hello
    // has no world of blocks to show.
    let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect();
    assert!(lines.iter().any(|line| line == "view 1.4 False True True"), "{lines:?}");
    let map = game.engine.world.get::<Map>(obj).unwrap().clone();
    assert_eq!((map.pointer_control, map.map_size), (false, 1.3));
    let number = map.views.expect("its views are on the screen")[1];
    let wish = script.wishes.get(&number).expect("what the script asked of its map").clone();
    assert!((wish.turn.unwrap() - std::f32::consts::FRAC_PI_2).abs() < 1e-4);
    assert_eq!((wish.zoom, wish.look_at), (Some(6.0), Some(Vec3::new(200.0, 0.0, 50.0))));
    let ui = game.ui.ui();
    let large = ui.map_views().into_iter().find(|view| ui.map_view_settings(*view).unwrap().kind == MapViewKind::Map);
    assert!(ui.is_visible(large.unwrap()), "the map shows once opened");
    // The element's settings took at once; the Map's own, the frame after, over the element's where both say.
    let settings = ui.map_view_settings(large.unwrap()).unwrap();
    assert_eq!((settings.fill, settings.compass, settings.zoom_range), (1.3, false, [1, 16]));
    assert!(!settings.controls, "the Map's Pointer Control is its map's");
}
