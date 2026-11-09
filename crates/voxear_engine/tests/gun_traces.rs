//! Gun traces: six players side by side, each with a body built here and
//! the Rifle in its hands, aiming, turning, walking, firing and reloading
//! from a clip, and one held by arms clips made here, as a gun from an
//! animation pack is. Each frame records where the gun, the eye, the hands, the
//! fingers and the gun's moving parts are, and what the gun has in it as a
//! script reads it. The traces are kept with the guns package, and a run
//! must match them: so guns written again elsewhere hold, move and work as
//! these did.
//!
//! With `VOXEAR_RECORD_GUN_TRACES=1` the traces are written, into
//! `VOXEAR_GUN_TRACES` or else the guns package's `Tests/Traces` beside the
//! engine's folder. Without traces there, it skips itself, as it does
//! without .NET and the managed build.
//!
//! With `VOXEAR_GUNS_PACKAGE` naming the guns package's folder, the guns
//! are the package's instead of the engine's: the Hello copy adds it as a
//! folder package, the Guns module stays off, the driver uses its
//! namespace, and the Rifle is the package's. They must match the same
//! traces, which are never recorded from them. `VOXEAR_GUN_TRACES_RUN`
//! names a folder to write a run's own traces into, to look over beside
//! the kept ones.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use voxear_core::logging::LogCapture;
use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_engine::animation::{ClipSource, Clips};
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_render::{Bone, BonePose, Camera, Clip, ModelAnimation, ModelRenderer, Pose, Skeleton};
use voxear_scene::{Name, Parent, ScenePersistence, Transform, world_matrix};

/// What each player does, by the name the driver knows it by.
const TRACES: [&str; 6] = ["Aim", "Turn", "Walk", "Fire", "Reload", "Clips"];

/// How many frames each trace runs, at 60 a second: long enough for a
/// clip reload to end.
const FRAMES: usize = 720;

/// How far a value may be from its trace's: a millimetre, or a
/// thousandth of a quaternion's part.
const CLOSE: f32 = 1e-3;

const DRIVER: &str = r#"using System.Globalization;
using System.Numerics;
using Voxear;

namespace Hello;

/// <summary>
/// Drives the gun traces: each player arms itself with the Rifle and does what its trace is of, and says each frame
/// what its gun has in it and what its holder is doing.
/// </summary>
public sealed class GunTrace : SystemBase
{
	private static readonly string[] Traces = ["Aim", "Turn", "Walk", "Fire", "Reload", "Clips"];

	private int frame;

	protected override void Update()
	{
		// The players come once the level has loaded.
		if (!Obj.Find("Player Aim").IsValid)
		{
			return;
		}
		frame++;
		foreach (var trace in Traces)
		{
			var player = Obj.Find($"Player {trace}");
			if (frame == 1)
			{
				Arm(player);
			}
			var did = Act(trace, player);
			Log.Info(Said(trace, player, did));
		}
	}

	private static void Arm(Obj player)
	{
		player.AddFirstPersonView().MouseLook = false;
		var rifle = Prefab.Load("Rifle.prefab").Instantiate(player);
		var gun = rifle.Gun;
		if (player.Name == "Player Clips")
		{
			gun.Handling = HeldByClips();
		}
		player.AddGunHolder().Gun = gun;
	}

	/// <summary>The handling whose arms clips, made by the test, hold the gun.</summary>
	private static GunHandling HeldByClips()
	{
		return GunHandling.Load("Guns/Clips.gunhandling");
	}

	private string Act(string trace, Obj player)
	{
		var holder = player.GunHolder;
		switch (trace)
		{
			case "Aim":
				holder.Aiming = frame >= 40 && frame < 160;
				return "";
			case "Turn":
				var view = player.FirstPersonView;
				view.Yaw += frame >= 30 && frame < 60 ? 3f : 0f;
				view.Pitch += frame >= 90 && frame < 105 ? -2f : 0f;
				return "";
			case "Walk":
				player.Position += frame >= 30 && frame < 150 ? new Vector3(0f, 0f, 1.5f / 60f) : Vector3.Zero;
				return "";
			case "Fire":
				return frame == 60 || frame == 70 || frame == 240 ? Shot(holder) : "";
			case "Reload":
				return Reload(holder);
			default:
				return HoldByClips(player, holder);
		}
	}

	/// <summary>Held by clips: aimed, run with, turned, fired, and reloaded from a clip by hand.</summary>
	private string HoldByClips(Obj player, GunHolder holder)
	{
		holder.Aiming = frame >= 60 && frame < 180;
		player.Position += frame >= 240 && frame < 330 ? new Vector3(0f, 0f, 3f / 60f) : Vector3.Zero;
		var view = player.FirstPersonView;
		view.Yaw += frame >= 400 && frame < 430 ? 2f : 0f;
		if (frame == 520)
		{
			var gun = holder.Gun;
			gun.Rounds = 0;
		}
		if (frame == 530)
		{
			holder.Reload();
		}
		return frame == 450 ? Shot(holder) : "";
	}

	private string Reload(GunHolder holder)
	{
		if (frame == 20)
		{
			var gun = holder.Gun;
			gun.Rounds = 0;
		}
		if (frame == 30)
		{
			holder.Reload();
		}
		return "";
	}

	private static string Shot(GunHolder holder)
	{
		var fired = holder.Fire(out var shot);
		var way = shot.Direction;
		return string.Create(CultureInfo.InvariantCulture, $" shot {fired} {way.X:F4} {way.Y:F4} {way.Z:F4}");
	}

	private string Said(string trace, Obj player, string did)
	{
		var holder = player.GunHolder;
		var gun = holder.Gun;
		var acting = holder.Acting?.ToString() ?? "-";
		return string.Create(
			CultureInfo.InvariantCulture,
			$"trace {trace} {frame} {gun.Rounds} {gun.Chambered} {gun.IsOpen} {holder.Spare} {acting} {holder.Aimed:F3}{did}");
	}
}
"#;

/// The Rifle the editor makes, as a prefab: a bolt-action Gun with blocks
/// for its parts, its bolt turning up and sliding back, a stripper clip
/// and a case.
const RIFLE: &str = r#"format = 1

[[obj]]
id = 1
name = "Rifle"
[obj.gun]
kind = "bolt_action_rifle"

[[obj]]
id = 2
name = "Stock"
parent = 1
position = [0.0, -0.035, -0.28]
scale = [0.042, 0.13, 0.16]
[obj.renderer]
mesh = "builtin:cube"

[[obj]]
id = 3
name = "Fore-end"
parent = 1
position = [0.0, -0.005, 0.32]
scale = [0.045, 0.06, 0.72]
[obj.renderer]
mesh = "builtin:cube"

[[obj]]
id = 4
name = "Receiver"
parent = 1
position = [0.0, 0.04, 0.08]
scale = [0.034, 0.034, 0.22]
[obj.renderer]
mesh = "builtin:cube"

[[obj]]
id = 5
name = "Barrel"
parent = 1
position = [0.0, 0.045, 0.585]
scale = [0.018, 0.018, 0.81]
[obj.renderer]
mesh = "builtin:cube"

[[obj]]
id = 6
name = "Bolt"
parent = 1
position = [0.045, 0.05, 0.1]
scale = [0.06, 0.015, 0.015]
[obj.renderer]
mesh = "builtin:cube"
[obj.gun_part]
role = "bolt"
handle = [0.075, 0.05, 0.1]
motion = { type = "turn", axis = [0.0, 0.0, 1.0], pivot = [0.0, 0.05, 0.0], angle = 80.0 }
then = { type = "slide", axis = [0.0, 0.0, -1.0], travel = 0.1 }

[[obj]]
id = 7
name = "Rear Sight"
parent = 1
position = [0.0, 0.065, 0.2]
scale = [0.03, 0.015, 0.01]
[obj.renderer]
mesh = "builtin:cube"

[[obj]]
id = 8
name = "Front Sight"
parent = 1
position = [0.0, 0.07, 0.96]
scale = [0.006, 0.03, 0.006]
[obj.renderer]
mesh = "builtin:cube"

[[obj]]
id = 9
name = "Clip"
parent = 1
position = [0.0, 0.1, 0.12]
scale = [0.012, 0.05, 0.06]
[obj.renderer]
mesh = "builtin:cube"
[obj.gun_part]
role = "clip"
handle = [0.0, 0.13, 0.12]
motion = { type = "slide", axis = [0.0, -1.0, 0.0], travel = 0.25 }

[[obj]]
id = 10
name = "Case"
parent = 1
position = [0.03, 0.05, 0.12]
scale = [0.011, 0.011, 0.05]
[obj.renderer]
mesh = "builtin:cube"
[obj.gun_part]
role = "case"
handle = [0.03, 0.05, 0.12]
"#;

/// Frames of a trace left out of the comparison. The engine's guns take up a handling given during a frame from the
/// next frame on, the package's at once: so the Clips player's gun is held by its clips from its second frame in the
/// engine's trace, and from its first in the package's.
const UNSETTLED: [(&str, usize); 1] = [("Clips", 1)];

/// The clips that hold the Clips player's gun, in a handling: standing, walking, running, and starting a run, with the
/// eyes the clips were made for, the players' own.
const HELD_BY: &str = "carrier = \"Carrier\"\nidle = \"Arms#clip:Hold\"\nwalk = \"Arms#clip:Walk\"\nrun = \"Arms#clip:Run\"\n\
                       run_start = \"Arms#clip:Start\"\neyes = [0.0, 0.07, 0.11]\n";

/// What a trace records each frame, in order: the gun's place and turn in
/// the world, the eye's, each hand's, each index fingertip's place, and
/// the bolt's and the clip's on the gun.
const COLUMNS: &str = "gun.x gun.y gun.z gun.qx gun.qy gun.qz gun.qw \
    eye.x eye.y eye.z eye.qx eye.qy eye.qz eye.qw \
    hand_l.x hand_l.y hand_l.z hand_l.qx hand_l.qy hand_l.qz hand_l.qw \
    hand_r.x hand_r.y hand_r.z hand_r.qx hand_r.qy hand_r.qz hand_r.qw \
    index_l.x index_l.y index_l.z index_r.x index_r.y index_r.z \
    bolt.x bolt.y bolt.z bolt.qx bolt.qy bolt.qz bolt.qw clip.x clip.y clip.z";

/// A player's Objs: the one holding, its body and its eye.
struct Player {
    holder: Entity,
    body: Entity,
    rest: Vec<Mat4>,
}

#[test]
fn guns_hold_move_and_work_as_their_traces_have_them() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let recording = std::env::var_os("VOXEAR_RECORD_GUN_TRACES").is_some();
    let package = std::env::var_os("VOXEAR_GUNS_PACKAGE").map(PathBuf::from);
    assert!(!(recording && package.is_some()), "the traces are recorded from the engine's guns, never the package's");
    let folder = traces_folder(package.as_deref());
    if !recording && !folder.join("Aim.trace").is_file() {
        return eprintln!("skipping: no gun traces in {}", folder.display());
    }
    let traces = run(package.as_deref());
    if recording {
        std::fs::create_dir_all(&folder).unwrap();
        for (name, lines) in &traces {
            std::fs::write(folder.join(format!("{name}.trace")), trace_file(name, lines)).unwrap();
        }
        return eprintln!("recorded {} gun traces in {}", traces.len(), folder.display());
    }
    // A run's own traces, to look over beside the kept ones, where `VOXEAR_GUN_TRACES_RUN` names a folder.
    if let Some(run_folder) = std::env::var_os("VOXEAR_GUN_TRACES_RUN").map(PathBuf::from) {
        std::fs::create_dir_all(&run_folder).unwrap();
        for (name, lines) in &traces {
            std::fs::write(run_folder.join(format!("{name}.trace")), trace_file(name, lines)).unwrap();
        }
    }
    let mut differences = Vec::new();
    for (name, lines) in &traces {
        let kept = std::fs::read_to_string(folder.join(format!("{name}.trace"))).unwrap();
        differences.extend(compare(name, &kept, lines));
    }
    assert!(differences.is_empty(), "the guns differ from their traces:\n{}", differences.join("\n"));
}

/// Runs every trace at once in a copy of Hello, with the Guns module on or
/// the guns package in `package`, and gives each one's lines, a frame a line.
fn run(package: Option<&Path>) -> BTreeMap<String, Vec<String>> {
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::create_dir_all(dir.path().join("Guns")).unwrap();
    let project = match package {
        Some(package) => with_package(dir.path(), package),
        None => {
            std::fs::write(dir.path().join("Scripts/GunTrace.cs"), DRIVER).unwrap();
            std::fs::write(dir.path().join("Rifle.prefab"), RIFLE).unwrap();
            std::fs::write(dir.path().join("Guns/Clips.gunhandling"), format!("[clips]\n{HELD_BY}")).unwrap();
            let mut project = Project::open(dir.path()).unwrap();
            project.set_module(voxear_engine::modules::Module::Guns, true).unwrap();
            project
        }
    };
    if let Err(error) = scripts::build(&project) {
        let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect();
        panic!("{error:#}: {lines:#?}");
    }
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    // The level loads on the first frame, in place of what was there.
    game.tick(1.0 / 60.0, None);
    let arms = arms_clips();
    let clips = game.engine.world.resource_mut::<Clips>().expect("the game's clips");
    for (index, clip) in arms.clips.iter().enumerate() {
        let source = ClipSource { animation: arms.clone(), index, settings: Default::default() };
        clips.put(&format!("Arms#clip:{}", clip.name), source);
    }
    let players: Vec<Player> =
        TRACES.iter().enumerate().map(|(at, name)| spawn_player(&mut game.engine.world, name, at)).collect();
    let mut numbers: BTreeMap<(String, usize), String> = BTreeMap::new();
    for frame in 1..=FRAMES {
        // The body stands at rest each frame before the gun is held, as an Animator would pose it.
        for player in &players {
            game.engine.world.insert(player.body, Pose { bones: player.rest.clone().into() });
        }
        game.tick(1.0 / 60.0, None);
        for (name, player) in TRACES.iter().zip(&players) {
            numbers.insert((name.to_string(), frame), measure(&game.engine.world, player));
        }
    }
    let said = said_by_frame(&log);
    let mut traces: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for ((name, frame), measured) in numbers {
        let state = said.get(&(name.clone(), frame)).cloned().unwrap_or_else(|| "?".into());
        traces.entry(name).or_default().push(format!("{frame} {measured} | {state}"));
    }
    traces
}

/// The Hello copy in `dir` with the guns package in `package` as a folder
/// package, the driver using its guns and the Rifle the package's, and the
/// Guns module off.
fn with_package(dir: &Path, package: &Path) -> Project {
    let package = package.canonicalize().expect("the guns package's folder");
    let toml = format!("[packages]\nvoxear-guns = {{ path = {:?} }}\n", package.display().to_string());
    std::fs::write(dir.join("packages.toml"), toml).unwrap();
    let driver = DRIVER.replacen("using Voxear;\n", "using Voxear;\nusing Voxear.Guns;\n", 1);
    let driver = driver.replacen(
        "GunHandling.Load(\"Guns/Clips.gunhandling\")",
        "Definition.Load<GunHandling>(\"Guns/Clips.asset\")!",
        1,
    );
    std::fs::write(dir.join("Scripts/GunTrace.cs"), driver).unwrap();
    let handling = format!("type = \"Voxear.Guns.GunHandling\"\n\n[fields.clips]\n{HELD_BY}");
    std::fs::write(dir.join("Guns/Clips.asset"), handling).unwrap();
    std::fs::copy(package.join("Prefabs/Rifle.prefab"), dir.join("Rifle.prefab")).unwrap();
    let mut project = Project::open(dir).unwrap();
    project.set_module(voxear_engine::modules::Module::Guns, false).unwrap();
    project
}

/// A player away from Hello's own Objs and from the others: the Obj that
/// holds, facing +Z, with a body standing in it and an eye.
fn spawn_player(world: &mut World, name: &str, at: usize) -> Player {
    let position = Vec3::new(100.0 + 10.0 * at as f32, 0.0, 0.0);
    // Kept through Hello's change of level, as the players of a game are.
    let holder = world.spawn((Name(format!("Player {name}")), Transform::from_position(position), ScenePersistence));
    let skeleton = body();
    let rest = skeleton.model_matrices(&skeleton.rest());
    let animation = Arc::new(ModelAnimation { skeleton, skins: Vec::new(), clips: Vec::new() });
    let model = ModelRenderer { animation: Some(animation), ..ModelRenderer::new(Vec::new()) };
    let pose = Pose { bones: rest.clone().into() };
    let body = world.spawn((Name("Body".into()), Transform::IDENTITY, Parent(holder), model, pose));
    world.spawn((Name("Eye".into()), Transform::IDENTITY, Parent(holder), Camera::default()));
    Player { holder, body, rest }
}

/// One frame of a player's numbers, as [`COLUMNS`] lists them.
fn measure(world: &World, player: &Player) -> String {
    let child = |parent: Entity, name: &str| {
        world
            .query_ref::<(Entity, &Name, &Parent)>()
            .into_iter()
            .find(|(_, found, under)| found.0 == name && under.0 == parent)
            .map(|(entity, _, _)| entity)
    };
    let mut values: Vec<f32> = Vec::new();
    let mut place = |matrix: Mat4, turn: bool| {
        let (_, rotation, translation) = matrix.to_scale_rotation_translation();
        values.extend(translation.to_array());
        if turn {
            values.extend(signed(rotation).to_array());
        }
    };
    let gun = child(player.holder, "Rifle");
    place(gun.map_or(Mat4::IDENTITY, |gun| world_matrix(world, gun)), true);
    let eye = child(player.holder, "Eye").expect("the eye");
    place(world_matrix(world, eye), true);
    let model = world_matrix(world, player.body);
    let skeleton = &world.get::<ModelRenderer>(player.body).unwrap().animation.clone().unwrap().skeleton;
    let bones = world.get::<Pose>(player.body).map(|pose| pose.bones.to_vec()).unwrap_or_default();
    let bone = |name: &str| {
        let index = skeleton.bones.iter().position(|bone| bone.name == name).unwrap();
        model * bones.get(index).copied().unwrap_or(Mat4::IDENTITY)
    };
    place(bone("hand_l"), true);
    place(bone("hand_r"), true);
    place(bone("index_02_l"), false);
    place(bone("index_02_r"), false);
    let local = |name: &str| {
        let part = gun.and_then(|gun| child(gun, name));
        part.and_then(|part| world.get::<Transform>(part).map(|at| at.matrix())).unwrap_or(Mat4::IDENTITY)
    };
    place(local("Bolt"), true);
    place(local("Clip"), false);
    values.iter().map(|value| format!("{:.4}", value + 0.0)).collect::<Vec<_>>().join(" ")
}

/// A turn written one way of its two: `q` and `-q` are the same turn.
fn signed(turn: Quat) -> Quat {
    if turn.w < 0.0 { -turn } else { turn }
}

/// What the driver said each frame of each trace, after the frame's number.
fn said_by_frame(log: &LogCapture) -> BTreeMap<(String, usize), String> {
    let mut said = BTreeMap::new();
    for line in log.lock().unwrap().lines.iter() {
        let Some(rest) = line.message.strip_prefix("trace ") else { continue };
        let mut words = rest.splitn(3, ' ');
        let (Some(name), Some(frame), Some(state)) = (words.next(), words.next(), words.next()) else { continue };
        if let Ok(frame) = frame.parse::<usize>() {
            said.insert((name.to_string(), frame), state.to_string());
        }
    }
    said
}

/// A trace's file: what it is, its columns, and a line a frame.
fn trace_file(name: &str, lines: &[String]) -> String {
    let mut text = format!(
        "# The gun trace {name}, recorded from the engine's guns at 60 frames a second.\n\
         # Each line: the frame, then {COLUMNS}, then after | what the driver said:\n\
         # rounds, chambered, open, spare, the action under way, how far aimed, and any shot.\n"
    );
    for line in lines {
        text.push_str(line);
        text.push('\n');
    }
    text
}

/// Where a run differs from a kept trace: the first few frames, each with
/// the column that differs most.
fn compare(name: &str, kept: &str, run: &[String]) -> Vec<String> {
    let columns: Vec<&str> = COLUMNS.split_whitespace().collect();
    let kept: Vec<&str> = kept.lines().filter(|line| !line.starts_with('#')).collect();
    if kept.len() != run.len() {
        return vec![format!("{name}: {} frames kept, {} run", kept.len(), run.len())];
    }
    let mut found = Vec::new();
    for (kept, run) in kept.iter().zip(run) {
        let (kept_numbers, kept_said) = kept.split_once(" | ").unwrap_or((kept, ""));
        let (run_numbers, run_said) = run.split_once(" | ").unwrap_or((run, ""));
        let parse = |text: &str| text.split_whitespace().map(|word| word.parse::<f32>().unwrap_or(f32::NAN)).collect();
        let (kept_values, run_values): (Vec<f32>, Vec<f32>) = (parse(kept_numbers), parse(run_numbers));
        let frame = kept_values.first().copied().unwrap_or_default();
        if UNSETTLED.contains(&(name, frame as usize)) {
            continue;
        }
        let worst = kept_values[1..]
            .iter()
            .zip(&run_values[1..])
            .enumerate()
            .map(|(column, (kept, run))| (column, (kept - run).abs()))
            .filter(|(_, apart)| !(*apart <= CLOSE))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((column, apart)) = worst {
            let column = columns.get(column).copied().unwrap_or("?");
            found.push(format!("{name} frame {frame}: {column} is {apart:.4} from its trace"));
        }
        if kept_said != run_said {
            found.push(format!("{name} frame {frame}: said \"{run_said}\", the trace \"{kept_said}\""));
        }
        if found.len() >= 12 {
            break;
        }
    }
    found
}

/// The guns package's traces: in `VOXEAR_GUN_TRACES` when it names a
/// folder, or else in the package being run, or else in `VoxearGuns` beside
/// the engine's folder, or beside a folder it is in, as a work tree under it
/// is.
fn traces_folder(package: Option<&Path>) -> PathBuf {
    if let Some(folder) = std::env::var_os("VOXEAR_GUN_TRACES") {
        return PathBuf::from(folder);
    }
    if let Some(package) = package {
        return package.join("Tests/Traces");
    }
    let engine = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let engine = engine.canonicalize().unwrap_or(engine);
    let package = engine.ancestors().skip(1).map(|folder| folder.join("VoxearGuns")).find(|folder| folder.is_dir());
    package.unwrap_or_else(|| engine.join("../VoxearGuns")).join("Tests/Traces")
}

/// Arms clips of the players' own body, made here: its arms held forward to a gun carried by a `Carrier` bone
/// before its chest, standing (Hold), bobbing as it walks (Walk), a little higher running (Run), and lifted and
/// brought down to the run's height over half a second as a run starts (Start).
fn arms_clips() -> Arc<ModelAnimation> {
    let mut skeleton = body();
    let at = |translation| BonePose { translation, ..BonePose::default() };
    skeleton.bones.push(Bone { name: "Carrier".into(), parent: Some(0), rest: at(Vec3::new(0.12, 0.4, 0.45)) });
    let carrier = skeleton.bones.len() - 1;
    let find = |name: &str| skeleton.bones.iter().position(|bone| bone.name == name).unwrap();
    let mut held = skeleton.rest();
    held[find("upper_arm_r")].rotation = Quat::from_rotation_y(-80f32.to_radians()) * Quat::from_rotation_z(-0.3);
    held[find("upper_arm_l")].rotation = Quat::from_rotation_y(70f32.to_radians()) * Quat::from_rotation_z(0.3);
    held[find("forearm_r")].rotation = Quat::from_rotation_y(-20f32.to_radians());
    held[find("forearm_l")].rotation = Quat::from_rotation_y(30f32.to_radians());
    let clip = |name: &str, seconds: f32, up: &dyn Fn(f32) -> f32| {
        let frames = (seconds * voxear_render::CLIP_RATE).round() as usize + 1;
        let poses = (0..frames).flat_map(|frame| {
            let mut pose = held.clone();
            pose[carrier].translation.y += up(frame as f32 / voxear_render::CLIP_RATE);
            pose
        });
        Clip { name: name.into(), frames, poses: poses.collect() }
    };
    let clips = vec![
        clip("Hold", 1.0, &|_| 0.0),
        clip("Walk", 1.0, &|t| 0.02 * (std::f32::consts::TAU * t).sin()),
        clip("Run", 1.0, &|_| 0.05),
        clip("Start", 0.5, &|t| if t < 0.25 { 0.4 * t } else { 0.1 - 0.2 * (t - 0.25) }),
    ];
    Arc::new(ModelAnimation { skeleton, skins: Vec::new(), clips })
}

/// A humanoid standing at the Obj's middle facing +Z, its arms out to the
/// sides, with fingers to close on the gun.
fn body() -> Skeleton {
    let mut bones: Vec<Bone> = Vec::new();
    let mut add = |name: String, parent: Option<usize>, at: Vec3| {
        bones.push(Bone { name, parent, rest: BonePose { translation: at, ..BonePose::default() } });
        bones.len() - 1
    };
    let hips = add("hips".into(), None, Vec3::new(0.0, 1.0, 0.0));
    let spine = add("spine".into(), Some(hips), Vec3::new(0.0, 0.15, 0.0));
    let chest = add("chest".into(), Some(spine), Vec3::new(0.0, 0.2, 0.0));
    let neck = add("neck".into(), Some(chest), Vec3::new(0.0, 0.2, 0.0));
    add("head".into(), Some(neck), Vec3::new(0.0, 0.1, 0.0));
    for (side, out) in [("l", -1.0), ("r", 1.0)] {
        let clavicle = add(format!("shoulder_{side}"), Some(chest), Vec3::new(0.05 * out, 0.15, 0.0));
        let upper = add(format!("upper_arm_{side}"), Some(clavicle), Vec3::new(0.15 * out, 0.0, 0.0));
        let twist = add(format!("upper_arm_twist_{side}"), Some(upper), Vec3::new(0.14 * out, 0.0, 0.0));
        let forearm = add(format!("forearm_{side}"), Some(twist), Vec3::new(0.14 * out, 0.0, 0.0));
        let hand = add(format!("hand_{side}"), Some(forearm), Vec3::new(0.26 * out, 0.0, 0.0));
        let index = add(format!("index_01_{side}"), Some(hand), Vec3::new(0.08 * out, 0.0, 0.02));
        add(format!("index_02_{side}"), Some(index), Vec3::new(0.03 * out, 0.0, 0.0));
        add(format!("thumb_01_{side}"), Some(hand), Vec3::new(0.03 * out, -0.02, 0.04));
        add(format!("middle_01_{side}"), Some(hand), Vec3::new(0.085 * out, 0.0, 0.0));
        add(format!("pinky_01_{side}"), Some(hand), Vec3::new(0.075 * out, 0.0, -0.035));
    }
    for (side, out) in [("l", -1.0), ("r", 1.0)] {
        let thigh = add(format!("thigh_{side}"), Some(hips), Vec3::new(0.1 * out, -0.05, 0.0));
        let shin = add(format!("shin_{side}"), Some(thigh), Vec3::new(0.0, -0.45, 0.0));
        let foot = add(format!("foot_{side}"), Some(shin), Vec3::new(0.0, -0.42, 0.0));
        add(format!("toe_{side}"), Some(foot), Vec3::new(0.0, -0.06, 0.12));
    }
    Skeleton { bones }
}

/// A copy of Hello, inside the repository, where its scripts find the
/// engine to build against.
fn hello_copy() -> tempfile::TempDir {
    fn copy(from: &Path, to: &Path) {
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
    copy(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/Hello"), dir.path());
    dir
}
