//! Guns from animation packs, held by the guns package: a gun drawn by one
//! skinned model whose bones its own clips move, its bolt worked from clips
//! part by part, and guns held by arms clips made here from how the
//! package's own hold has a body. Every player stands side by side in one
//! copy of Hello, as .NET starts only once in a process: a driver script arms
//! each and does what its check asks, and the checks read the world here.
//!
//! The package is the folder `VOXEAR_GUNS_PACKAGE` names; without it, as
//! without .NET and the managed build, it skips itself.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use voxear_core::logging::LogCapture;
use voxear_core::math::{EulerRot, Mat3, Mat4, Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_engine::animation::ik::{PoseEdit, hand_axes};
use voxear_engine::animation::rig::{Arm, Rig};
use voxear_engine::animation::{ClipSource, Clips, grip};
use voxear_engine::first_person::FirstPersonView;
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_render::{
    Bone, BonePose, CLIP_RATE, Camera, Clip, HiddenBones, MaterialHandle, MeshHandle, MeshRenderer, ModelAnimation,
    ModelRenderer, Pose, Skeleton,
};
use voxear_scene::{Name, Parent, ScenePersistence, Transform, world_matrix};

const DRIVER: &str = r#"using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Numerics;
using System.Reflection;
using Voxear;
using Voxear.Guns;

namespace Hello;

/// <summary>Arms each player of the pack checks with a gun and its handling, and does what its check asks, frame by frame.</summary>
public sealed class PackDriver : SystemBase
{
	private static readonly string[] Players =
		["Own", "Fire", "Empty", "Held", "Shorter", "Relief", "Breath", "Twist", "TwistShort", "Filmed", "AtEyes", "Palms", "Alone", "Run", "Bobbed"];

	private static readonly Vector3 ViewEyes = new(0f, 0.07f, 0.11f);

	private readonly Dictionary<string, int> frames = [];
	private float relief;

	protected override void Update()
	{
		foreach (var name in Players)
		{
			var player = Obj.Find($"Player {name}");
			if (!player.IsValid)
			{
				continue;
			}
			var frame = frames[name] = frames.GetValueOrDefault(name) + 1;
			if (frame == 1)
			{
				Arm(name, player);
			}
			var holder = player.GunHolder!;
			var gun = holder.Gun!;
			Log.Info(string.Create(CultureInfo.InvariantCulture, $"said {name} {frame} {gun.Rounds} {gun.Chamber} {holder.Acting?.ToString() ?? "-"}"));
			Act(name, player, holder, gun, frame);
		}
	}

	private static void Arm(string name, Obj player)
	{
		var view = player.AddFirstPersonView();
		view.MouseLook = false;
		var gun = player.Children.First(child => child.Name == "Gun").AddGun();
		var holder = player.AddGunHolder();
		holder.Gun = gun;
		if (name is "Fire" or "Empty")
		{
			gun.Obj.Children.First(child => child.Name == "Case").AddGunPart().Make(PartRole.Case, new Vector3(0f, 0.05f, 0.12f));
			gun.Handling = PackRifle();
			return;
		}
		// Held still, with the head shown, so a hold can be compared and made into a clip.
		view.HideHead = false;
		(holder.Sway, holder.Bob, holder.Breathing, holder.WallCheck) = (0f, 0f, 0f, false);
		if (name == "Breath")
		{
			(holder.Bob, holder.Breathing) = (1f, 1f);
		}
		if (name == "Alone")
		{
			(holder.Sway, holder.Bob, holder.Breathing, holder.BaseOnly) = (1f, 1f, 1f, true);
		}
		gun.Handling = name switch
		{
			"Own" => null,
			"Twist" or "TwistShort" => HeldBy("Rolled#clip:Hold", PackCamera.Filmed),
			"AtEyes" => HeldBy("Arms#clip:Hold", PackCamera.Eyes),
			"Run" => Running(),
			_ => HeldBy("Arms#clip:Hold", PackCamera.Filmed),
		};
	}

	private void Act(string name, Obj player, GunHolder holder, Gun gun, int frame)
	{
		switch (name, frame)
		{
			case ("Fire", 6):
			case ("Held", 71):
				Log.Info($"fired {name} {holder.Fire()}");
				break;
			case ("Empty", 2):
				gun.Chamber = Chamber.Empty;
				holder.Do(GunAction.Cycle);
				break;
			case ("Held", 1):
				Snaps(player, "Snaps");
				Snaps(player, "Slow");
				break;
			case ("Held", 41):
				holder.Aiming = true;
				break;
			case ("Relief", 41):
				relief = RearSight(player, gun);
				gun.EyeRelief = relief - 0.15f;
				holder.Aiming = true;
				break;
			case ("Relief", 101):
				gun.EyeRelief = relief + 0.1f;
				break;
			case ("Palms", 41):
				holder.Sway = 1f;
				break;
		}
	}

	/// <summary>How far ahead of the eye the gun's rear sight is.</summary>
	private static float RearSight(Obj player, Gun gun)
	{
		var eye = player.Children.First(child => child.Name == "Eye");
		var sight = gun.Obj.WorldPosition + Vector3.Transform(gun.RearSight, gun.Obj.WorldRotation);
		return Vector3.Transform(sight - eye.WorldPosition, Quaternion.Inverse(eye.WorldRotation)).Z;
	}

	/// <summary>Where a clip snaps a hand back at its very end, as the package finds it.</summary>
	private static void Snaps(Obj player, string file)
	{
		var clip = new AnimationClip($"{file}#clip:Lifted");
		var length = player.Children.First(child => child.Name == "Body").Skeleton.ClipLength(clip) ?? 0f;
		var hold = typeof(GunHolder).Assembly.GetType("Voxear.Guns.ClipHold")!;
		object?[] given = [clip, length, new PackClips { Carrier = "Carrier" }, 0.35f, null];
		var read = (bool)hold.GetMethod("SnapsBackAt", BindingFlags.Static | BindingFlags.Public)!.Invoke(null, given)!;
		var at = (float?)given[4];
		Log.Info(string.Create(CultureInfo.InvariantCulture, $"snaps {file} {(read ? at?.ToString("F3") ?? "none" : "unread")}"));
	}

	private static GunHandling HeldBy(string clip, PackCamera camera)
	{
		// The clip was made with the holder's own eyes, which sit up and forward of its head.
		return new GunHandling { Pack = new PackClips { Idle = new AnimationClip(clip), Carrier = "Carrier", Eyes = ViewEyes, Camera = camera } };
	}

	private static GunHandling Running()
	{
		var pack = new PackClips
		{
			Idle = new AnimationClip("Idle#clip:Lifted"),
			Run = new AnimationClip("Run#clip:Lifted"),
			RunStart = new AnimationClip("Start#clip:Lifted"),
			Carrier = "Carrier",
			WalkSpeed = 0.5f,
			RunSpeed = 1f,
			Eyes = ViewEyes,
		};
		return new GunHandling { Pack = pack };
	}

	/// <summary>A bolt-action rifle drawn by one skinned model: its bolt worked from clips, the case thrown out and the next round chambered at marks.</summary>
	private static GunHandling PackRifle()
	{
		static Mark Mark(float at, GunEvent then) => new() { At = at, Event = then };
		var throws = new ClipPart { Gun = new AnimationClip("Gun#clip:Bolt"), Marks = [Mark(0.5f, GunEvent.Eject), Mark(0.8f, GunEvent.Chamber)], When = When.AllOf(Fact.ChamberFilled) };
		var empty = new ClipPart { Gun = new AnimationClip("Gun#clip:Bolt Up"), Marks = [Mark(0.8f, GunEvent.Chamber)], When = When.AllOf(Fact.ChamberEmpty) };
		var handling = new GunHandling { Pack = new PackClips { Rest = new AnimationClip("Gun#clip:Still"), GunFire = new AnimationClip("Gun#clip:Trigger") } };
		handling.Cycle.Mode = ActionMode.Clips;
		handling.Cycle.Parts.AddRange([throws, empty]);
		return handling;
	}
}
"#;

/// The pack rifle's bones: its root, its bolt and its trigger.
const BOLT: usize = 1;
const TRIGGER: usize = 2;

/// The players: their names, the bodies they stand in, and which tick of the
/// run they come in at, once the clips they are held by are made.
struct Player {
    name: &'static str,
    holder: Entity,
    body: Entity,
    camera: Entity,
    gun: Entity,
    animation: Arc<ModelAnimation>,
    rest: Vec<Mat4>,
    frame: usize,
}

/// What the Own player's hold was, made into clips: where it shows the gun, its palms and knuckles on the gun,
/// its index finger's bends and joints' ways, and the rolled clip's roll.
struct Own {
    shown: Mat4,
    palms: [Vec3; 2],
    knuckles: [Vec3; 2],
    curled: [[f32; 3]; 5],
    ways: [[Vec3; 3]; 5],
    roll: f32,
}

#[test]
fn the_guns_package_holds_guns_from_animation_packs_as_the_engines_guns_did() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let Some(package) = std::env::var_os("VOXEAR_GUNS_PACKAGE").map(PathBuf::from) else {
        return eprintln!("skipping: VOXEAR_GUNS_PACKAGE names no guns package");
    };
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    let project = with_package(dir.path(), &package);
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
    let mut failed = Checks::default();
    run(&mut game, &log, &mut failed);
    for (check, problems) in &failed.by_check {
        let state = if problems.is_empty() { "ok".to_string() } else { problems.join("; ") };
        eprintln!("{check}: {state}");
    }
    let problems: Vec<String> = failed.by_check.values().flatten().cloned().collect();
    assert!(problems.is_empty(), "{problems:#?}");
}

/// What each check found wrong, by its name.
#[derive(Default)]
struct Checks {
    by_check: BTreeMap<&'static str, Vec<String>>,
}

impl Checks {
    fn check(&mut self, check: &'static str, ok: bool, what: String) {
        let problems = self.by_check.entry(check).or_default();
        if !ok {
            problems.push(what);
        }
    }
}

/// Runs every check: the pack rifles and the Own hold first, then once the
/// Own hold is made into clips, the players those clips hold the gun for.
fn run(game: &mut Game, log: &LogCapture, checks: &mut Checks) {
    let world = &mut game.engine.world;
    let gun_file = pack_rifle_file();
    put_clips(world, "Gun", &gun_file);
    let mut players = vec![
        spawn(world, "Own", 0, body(dotted), None),
        spawn(world, "Fire", 1, body(dotted), Some(gun_file.clone())),
        spawn(world, "Empty", 2, body(dotted), Some(gun_file.clone())),
    ];
    let mut own = None;
    let mut late: Vec<Player> = Vec::new();
    let mut fire = FireRun::default();
    let mut empty = EmptyRun::default();
    let mut held = HeldRun::default();
    let mut walks = Walks::default();
    for tick in 0..240 {
        before(&mut game.engine.world, players.iter().chain(&late));
        game.tick(1.0 / 60.0, None);
        let world = &game.engine.world;
        for player in players.iter_mut().chain(late.iter_mut()) {
            player.frame += 1;
        }
        let said = said(log);
        fire.after(world, &players[1], &said, checks);
        empty.after(world, &players[2], &said, checks);
        if tick == 39 {
            let (made, files) = own_into_clips(&mut game.engine.world, &players[0]);
            for (name, file) in files {
                put_clips(&mut game.engine.world, name, &file);
            }
            late = spawn_held(&mut game.engine.world);
            own = Some(made);
            continue;
        }
        let Some(own) = own.as_ref() else { continue };
        held.after(world, &late, own, &said, checks);
        walks.after(world, &late, own, checks);
    }
    let said = said(log);
    let snaps = said.get(&("snaps Snaps".to_string(), 0)).cloned().unwrap_or_default();
    let found = snaps.parse::<f32>().ok();
    checks.check(
        "a_clip_that_snaps_a_hand_back_at_its_very_end_is_found_letting_go_as_the_hand_sets_off",
        found.is_some_and(|at| (at - 0.9).abs() < 0.03),
        format!("found as the hands set off: {snaps}"),
    );
    let slow = said.get(&("snaps Slow".to_string(), 0)).cloned().unwrap_or_default();
    checks.check(
        "a_clip_that_snaps_a_hand_back_at_its_very_end_is_found_letting_go_as_the_hand_sets_off",
        slow == "none",
        format!("a hand coming back slower than the Hand Return is left as the clip has it: {slow}"),
    );
}

/// Stands every body at rest before a frame, as an Animator would pose it,
/// and moves and turns the players that walk or look about, before the
/// frame, as each check has it.
fn before<'a>(world: &mut World, players: impl Iterator<Item = &'a Player>) {
    for player in players {
        let next = player.frame + 1;
        let mut pose = player.rest.clone();
        let view = |world: &mut World, change: &dyn Fn(&mut FirstPersonView)| {
            if let Some(view) = world.get_mut::<FirstPersonView>(player.holder) {
                change(view);
            }
        };
        let moved = |world: &mut World, by: f32| {
            world.get_mut::<Transform>(player.holder).unwrap().position.z += by;
        };
        match player.name {
            "Shorter" if next == 1 || next == 41 || next == 81 => {
                let pitch = [0.0, -55.0, 50.0][next / 40];
                view(world, &|view| view.pitch = pitch);
            }
            "Breath" if (41..=160).contains(&next) => moved(world, 3.0 / 60.0),
            "Run" if (41..=120).contains(&next) => moved(world, 3.0 / 60.0),
            "Palms" if (41..=100).contains(&next) => {
                view(world, &|view| view.yaw += if next < 71 { 4.0 } else { -4.0 })
            }
            "Alone" if (41..=130).contains(&next) => {
                moved(world, 3.0 / 60.0);
                view(world, &|view| view.yaw += if next < 86 { 3.0 } else { -3.0 });
            }
            "Bobbed" if (41..=100).contains(&next) => {
                // The body bobs four centimetres up and down and sways aside as a walk has it, a step every half second.
                let phase = (next - 41) as f32 / 30.0 * std::f32::consts::TAU;
                let bobbed = Mat4::from_translation(Vec3::new(phase.sin() * 0.02, (phase * 2.0).sin() * 0.02, 0.0));
                pose = pose.iter().map(|bone| bobbed * *bone).collect();
            }
            "Bobbed" if next == 101 => view(world, &|view| view.hide_head = true),
            "Bobbed" if next == 103 => {
                // The body's own idle turns its chest and drops a shoulder.
                let skeleton = &player.animation.skeleton;
                let rig = Rig::find(skeleton).unwrap();
                let mut swaying = skeleton.rest();
                swaying[rig.chest()].rotation = Quat::from_rotation_y(0.3) * Quat::from_rotation_x(0.1);
                swaying[rig.arms[0].clavicle.unwrap()].rotation = Quat::from_rotation_z(0.25);
                pose = skeleton.model_matrices(&swaying);
            }
            _ => {}
        }
        world.insert(player.body, Pose { bones: pose.into() });
    }
}

/// The pack rifle's shot and bolt, worked from clips.
#[derive(Default)]
struct FireRun {
    furthest: f32,
    thrown: Option<usize>,
    chambered: Option<usize>,
}

const FIRE: &str = "a_shot_plays_the_guns_fire_clip_then_its_bolt_is_worked_from_clips_marks_and_all";

impl FireRun {
    fn after(&mut self, world: &World, player: &Player, said: &Said, checks: &mut Checks) {
        let at_rest = gun_at_rest(world, player);
        let frame = player.frame;
        if frame == 5 {
            let off = gun_bone(world, player, BOLT).abs_diff_eq(at_rest[BOLT], 1e-5);
            checks.check(FIRE, off, "standing in its rest clip".into());
        }
        if frame == 20 {
            let pulled = gun_bone(world, player, TRIGGER).to_scale_rotation_translation().1;
            let angle = pulled.angle_between(Quat::IDENTITY).to_degrees();
            checks.check(FIRE, (angle - 40.0).abs() < 1.5, format!("the trigger pulled all the way: {angle}"));
        }
        if frame == 21 {
            let acting = said_of(said, player, 21).map(|s| s.acting.clone());
            checks.check(FIRE, acting.as_deref() == Some("-"), format!("the bolt waits for the fire clip: {acting:?}"));
            let fired = said.get(&("fired Fire".to_string(), 0)).cloned();
            checks.check(FIRE, fired.as_deref() == Some("True"), format!("it fired: {fired:?}"));
        }
        if (21..=120).contains(&frame) {
            let back = at_rest[BOLT].w_axis.z - gun_bone(world, player, BOLT).w_axis.z;
            self.furthest = self.furthest.max(back);
            if self.thrown.is_none() && cases(world) == 1 {
                self.thrown = Some(frame);
            }
        }
        if (22..=121).contains(&frame) && self.chambered.is_none() {
            self.chambered = said_of(said, player, frame).filter(|s| s.chamber == "Live").map(|_| frame - 1);
        }
        if frame == 121 {
            let furthest = self.furthest;
            checks.check(
                FIRE,
                (furthest - 0.08).abs() < 0.002,
                format!("the bolt went back as the clip takes it: {furthest}"),
            );
            let apart = self.thrown.zip(self.chambered).map(|(thrown, chambered)| chambered as i32 - thrown as i32);
            checks.check(
                FIRE,
                apart.is_some_and(|apart| (apart - 18).abs() <= 1),
                format!("0.3 s apart, as marked: {apart:?}"),
            );
            checks.check(FIRE, cases(world) == 1, format!("one case, thrown once: {}", cases(world)));
            let state = said_of(said, player, 121);
            let ready = state.as_ref().is_some_and(|s| s.chamber == "Live" && s.rounds == 4);
            checks.check(FIRE, ready, format!("ready, four left: {state:?}"));
            let home = gun_bone(world, player, BOLT).abs_diff_eq(at_rest[BOLT], 1e-5);
            checks.check(FIRE, home, "and back in its rest clip".into());
        }
    }
}

/// The pack rifle's bolt worked on an empty chamber.
#[derive(Default)]
struct EmptyRun {
    highest: f32,
}

const EMPTY: &str = "working_the_bolt_on_an_empty_chamber_plays_the_clip_that_throws_nothing";

impl EmptyRun {
    fn after(&mut self, world: &World, player: &Player, said: &Said, checks: &mut Checks) {
        let at_rest = gun_at_rest(world, player);
        let frame = player.frame;
        if (2..=31).contains(&frame) {
            self.highest = self.highest.max(gun_bone(world, player, BOLT).w_axis.y - at_rest[BOLT].w_axis.y);
        }
        if frame == 31 {
            let highest = self.highest;
            checks.check(EMPTY, highest > 0.015, format!("the bolt went up, as the no-round clip takes it: {highest}"));
            let view = world.get::<FirstPersonView>(player.holder).unwrap().rotation();
            let looks = world_matrix(world, player.camera).to_scale_rotation_translation().1;
            checks.check(EMPTY, looks.angle_between(view) < 1e-3, "the camera looks where the view does".into());
            checks.check(EMPTY, cases(world) == 0, format!("nothing thrown out: {}", cases(world)));
        }
        if frame == 32 {
            let acting = said_of(said, player, 32).is_some_and(|s| s.acting != "-");
            checks.check(EMPTY, acting, "working the bolt".into());
        }
        if frame == 72 {
            let state = said_of(said, player, 72);
            let chambered = state.as_ref().is_some_and(|s| s.chamber == "Live" && s.rounds == 4);
            checks.check(EMPTY, chambered, format!("chambered at its mark: {state:?}"));
        }
    }
}

const HELD: &str = "an_arms_clip_holds_the_gun_where_it_has_it_and_puts_the_palms_where_its_are";
const RELIEF: &str = "aimed_a_gun_held_by_clips_comes_up_no_nearer_than_they_hold_it";
const TWIST: &str = "an_arm_the_clips_hold_turns_as_theirs_does_twist_and_all";
const FILMED: &str = "seen_as_filmed_a_body_built_otherwise_holds_the_gun_as_the_clip_does_fingers_and_all";

/// The checks of guns held standing by the Own hold's clips, each at the
/// frames it is looked at.
#[derive(Default)]
struct HeldRun {
    held: Option<Mat4>,
    relief: f32,
    moved: Vec3,
}

impl HeldRun {
    fn after(&mut self, world: &World, players: &[Player], own: &Own, said: &Said, checks: &mut Checks) {
        let player = |name: &str| players.iter().find(|player| player.name == name).unwrap();
        let palms = |checks: &mut Checks, check, p: &Player, most: [f32; 2], what: &str| {
            for arm in 0..2 {
                let off = palm_on_gun(world, p, arm).distance(own.palms[arm]);
                checks.check(check, off < most[arm], format!("{what}, palm {arm}: {off} m off"));
            }
        };
        let held = player("Held");
        match held.frame {
            40 => {
                let shown = gun_from_eye(world, held);
                self.held = Some(shown);
                let off = shown.w_axis.truncate().distance(own.shown.w_axis.truncate());
                checks.check(HELD, off < 0.003, format!("the gun where the clip carries it: {off} m off"));
                palms(checks, HELD, held, [0.005; 2], "on the same body");
                let (index, clip_index) = (fingers(world, held, 1)[1], own.curled[1]);
                checks.check(HELD, clip_index[0] > 0.3, format!("the clip's trigger finger is bent: {clip_index:?}"));
                for joint in 0..2 {
                    let off = (index[joint] - clip_index[joint]).abs().to_degrees();
                    checks.check(
                        HELD,
                        off < 3.0,
                        format!("the index finger bends as the clip's, joint {joint}: {off} degrees off"),
                    );
                }
            }
            70 => {
                let up = gun_from_eye(world, held).w_axis.truncate().distance(self.held.unwrap().w_axis.truncate());
                checks.check(HELD, up > 0.05, format!("up to the eye: {up}"));
            }
            73 => {
                let fired = said.get(&("fired Held".to_string(), 0)).cloned();
                checks.check(HELD, fired.as_deref() == Some("True"), format!("it fired: {fired:?}"));
                palms(checks, HELD, held, [0.006; 2], "aimed and kicking");
            }
            _ => {}
        }
        let shorter = player("Shorter");
        if [40, 80, 120].contains(&shorter.frame) {
            palms(checks, HELD, shorter, [0.005; 2], &format!("on the shorter body, frame {}", shorter.frame));
        }
        if shorter.frame == 40 {
            let off = gun_from_eye(world, shorter).w_axis.truncate().distance(own.shown.w_axis.truncate() * 0.9);
            checks.check(HELD, off < 0.003, format!("the gun where the clip shows it, a tenth nearer: {off} m off"));
        }
        let relief = player("Relief");
        let rear = Vec3::new(0.0, 0.07, 0.2);
        match relief.frame {
            40 => self.relief = gun_from_eye(world, relief).transform_point3(rear).z,
            100 => {
                let sighted = gun_from_eye(world, relief).transform_point3(rear);
                checks.check(
                    RELIEF,
                    sighted.truncate().length() < 0.003,
                    format!("the rear sight on the camera's line: {sighted}"),
                );
                let held = self.relief;
                checks.check(
                    RELIEF,
                    (sighted.z - held).abs() < 0.005,
                    format!("as far out as the clips hold it, {held} m: {sighted}"),
                );
            }
            160 => {
                let sighted = gun_from_eye(world, relief).transform_point3(rear);
                let wanted = self.relief + 0.1;
                checks.check(
                    RELIEF,
                    (sighted.z - wanted).abs() < 0.005,
                    format!("at the eye relief, {wanted} m: {sighted}"),
                );
            }
            _ => {}
        }
        for name in ["Twist", "TwistShort"] {
            let twisted = player(name);
            if twisted.frame == 40 {
                let off = (body_roll(world, twisted) - own.roll).to_degrees().abs();
                checks.check(TWIST, off < 3.0, format!("{name}: the forearm rolls as the clip's: {off} degrees off"));
                palms(checks, TWIST, twisted, [0.005; 2], name);
            }
        }
        let filmed = player("Filmed");
        if filmed.frame == 60 {
            palms(checks, FILMED, filmed, [0.005; 2], "seen as filmed");
            let off = gun_from_eye(world, filmed).w_axis.truncate().distance(own.shown.w_axis.truncate());
            checks.check(FILMED, off < 0.04, format!("the gun shows near where the clip shows it: {off} m off"));
            let ways = finger_ways(world, filmed, 1);
            for joint in 0..2 {
                let off = ways[1][joint].angle_between(own.ways[1][joint]).to_degrees();
                checks.check(
                    FILMED,
                    off < 3.0,
                    format!("the index finger's joint {joint} points as the clip's: {off} degrees off"),
                );
            }
            self.moved = camera_from_head(world, filmed);
        }
        let eyes = player("AtEyes");
        if eyes.frame == 60 {
            let at_eyes = camera_from_head(world, eyes);
            let view = Vec3::from(FirstPersonView::default().eyes);
            checks.check(FILMED, at_eyes.distance(view) < 0.01, format!("at the eyes: {at_eyes} {view}"));
            checks.check(
                FILMED,
                self.moved.distance(view) > 0.01,
                format!("as filmed, it moved: {} {view}", self.moved),
            );
            let off = gun_from_eye(world, eyes).w_axis.truncate().distance(own.shown.w_axis.truncate());
            checks.check(FILMED, off < 0.003, format!("at the eyes, the gun where the clip shows it: {off} m off"));
            palms(checks, FILMED, eyes, [0.02, 0.005], "at the eyes");
        }
    }
}

const BREATH: &str = "a_gun_held_by_clips_neither_bobs_nor_breathes_as_its_clips_do_that_themselves";
const PALMS: &str = "hands_with_longer_palms_close_where_the_clips_do_and_hold_on_through_a_fast_turn";
const ALONE: &str = "shown_as_its_clips_alone_have_it_the_gun_stays_where_they_show_it_walking_and_turning";
const RUN: &str = "setting_off_into_a_run_plays_its_start_clip_then_the_run_from_its_first_frame";
const BOBBED: &str = "the_camera_stays_put_on_the_body_while_clips_hold_the_gun_however_the_body_bobs";

/// The checks of guns held by clips as the holder walks, turns and bobs:
/// the most each moves from where it should be along the way.
#[derive(Default)]
struct Walks {
    still: Vec3,
    breathing: f32,
    worst_knuckles: f32,
    alone: f32,
    standing: f32,
    highest: f32,
    camera: Vec3,
    bobbing: f32,
    turns: Option<[Quat; 2]>,
}

impl Walks {
    fn after(&mut self, world: &World, players: &[Player], own: &Own, checks: &mut Checks) {
        let player = |name: &str| players.iter().find(|player| player.name == name).unwrap();
        let breath = player("Breath");
        match breath.frame {
            40 => self.still = gun_from_eye(world, breath).w_axis.truncate(),
            41..=160 => {
                self.breathing = self.breathing.max(gun_from_eye(world, breath).w_axis.truncate().distance(self.still))
            }
            _ => {}
        }
        if breath.frame == 160 {
            let most = self.breathing;
            checks.check(
                BREATH,
                most < 0.0005,
                format!("the gun stays where the clip has it before the eyes: {most} m off"),
            );
        }
        let palms = player("Palms");
        if (41..=100).contains(&palms.frame) {
            for arm in 0..2 {
                self.worst_knuckles =
                    self.worst_knuckles.max(knuckles_on_gun(world, palms, arm).distance(own.knuckles[arm]));
            }
        }
        if palms.frame == 100 {
            let worst = self.worst_knuckles;
            checks.check(
                PALMS,
                worst < 0.006,
                format!("the knuckles where the clip's are, all through the turn: {worst} m off at worst"),
            );
        }
        let alone = player("Alone");
        if (41..=130).contains(&alone.frame) {
            self.alone =
                self.alone.max(gun_from_eye(world, alone).w_axis.truncate().distance(own.shown.w_axis.truncate()));
        }
        if alone.frame == 130 {
            let most = self.alone;
            checks.check(
                ALONE,
                most < 0.0005,
                format!("the gun where the clip shows it, all along: {most} m off at most"),
            );
        }
        let run = player("Run");
        match run.frame {
            40 => self.standing = gun_from_eye(world, run).w_axis.y,
            41..=120 => self.highest = self.highest.max(gun_from_eye(world, run).w_axis.y - self.standing),
            _ => {}
        }
        if run.frame == 120 {
            let highest = self.highest;
            checks.check(RUN, highest > 0.17, format!("the start lifts the gun as high as it does: {highest} m"));
            let running = gun_from_eye(world, run).w_axis.y - self.standing;
            checks.check(
                RUN,
                (running - 0.1).abs() < 0.01,
                format!("then the run holds it where the start left it: {running} m"),
            );
        }
        self.bobbed(world, player("Bobbed"), checks);
    }

    fn bobbed(&mut self, world: &World, bobbed: &Player, checks: &mut Checks) {
        let camera = || {
            let obj = world_matrix(world, bobbed.holder);
            obj.inverse().transform_point3(world_matrix(world, bobbed.camera).w_axis.truncate())
        };
        let rig = Rig::find(&bobbed.animation.skeleton).unwrap();
        let (chest, clavicle) = (rig.chest(), rig.arms[0].clavicle.unwrap());
        let turns = || {
            let posed = &world.get::<Pose>(bobbed.body).unwrap().bones;
            [chest, clavicle].map(|bone| posed[bone].to_scale_rotation_translation().1)
        };
        match bobbed.frame {
            40 => self.camera = camera(),
            41..=100 => self.bobbing = self.bobbing.max(camera().distance(self.camera)),
            101 => {
                let most = self.bobbing;
                checks.check(
                    BOBBED,
                    most < 0.001,
                    format!("the camera where it was on the body: {most} m off at most"),
                );
                // With the head hidden, the upper chest and collarbones round the low camera go too, but not the arms.
                let hidden =
                    world.get::<HiddenBones>(bobbed.body).map(|hidden| hidden.bones.clone()).unwrap_or_default();
                let at = |bone: usize| hidden.get(bone).copied().unwrap_or(false);
                let clavicles = rig.arms.iter().filter_map(|arm| arm.clavicle).all(at);
                checks.check(
                    BOBBED,
                    at(rig.head) && at(rig.chest()) && clavicles,
                    "the head, chest and collarbones hidden".into(),
                );
                checks.check(
                    BOBBED,
                    rig.arms.iter().all(|arm| !at(arm.upper) && !at(arm.hand)),
                    "the arms stay".into(),
                );
            }
            102 => self.turns = Some(turns()),
            103 => {
                // While the clips hold the gun the upper body stands as their stance has it, whatever the body's idle.
                for (now, was) in turns().into_iter().zip(self.turns.unwrap()) {
                    let off = now.angle_between(was).to_degrees();
                    checks.check(BOBBED, now.angle_between(was) < 0.01, format!("stands as before: {off} degrees off"));
                }
            }
            _ => {}
        }
    }
}

/// The players the Own hold's clips hold the gun for, each in its body.
fn spawn_held(world: &mut World) -> Vec<Player> {
    let shorter = || {
        let mut skeleton = body(prefixed);
        skeleton.bones.iter_mut().for_each(|bone| bone.rest.translation *= 0.9);
        skeleton
    };
    let mut palms = shorter();
    let hands: Vec<usize> = (0..palms.bones.len()).filter(|&bone| palms.bones[bone].name.contains("hand")).collect();
    for bone in palms.bones.iter_mut().filter(|bone| bone.parent.is_some_and(|parent| hands.contains(&parent))) {
        bone.rest.translation *= 1.25;
    }
    let mut filmed = turned(body(prefixed));
    filmed.bones.iter_mut().for_each(|bone| bone.rest.translation *= 0.9);
    let bodies = [
        ("Held", body(dotted)),
        ("Shorter", shorter()),
        ("Relief", body(dotted)),
        ("Breath", body(dotted)),
        ("Twist", body(dotted)),
        ("TwistShort", shorter()),
        ("Filmed", filmed.clone()),
        ("AtEyes", filmed),
        ("Palms", palms),
        ("Alone", body(dotted)),
        ("Run", body(dotted)),
        ("Bobbed", body(dotted)),
    ];
    bodies.into_iter().enumerate().map(|(at, (name, skeleton))| spawn(world, name, 3 + at, skeleton, None)).collect()
}

/// A player away from Hello's own Objs and from the others: the Obj that
/// holds, facing +Z, with a body standing in it, an eye, and a gun, drawn by
/// `gun_file`'s skinned model with a case under it for a pack rifle.
fn spawn(
    world: &mut World,
    name: &'static str,
    at: usize,
    skeleton: Skeleton,
    gun_file: Option<Arc<ModelAnimation>>,
) -> Player {
    let position = Vec3::new(100.0 + 10.0 * at as f32, 0.0, 0.0);
    let holder = world.spawn((Name(format!("Player {name}")), Transform::from_position(position), ScenePersistence));
    let rest = skeleton.model_matrices(&skeleton.rest());
    let animation = Arc::new(ModelAnimation { skeleton, skins: Vec::new(), clips: Vec::new() });
    let model = ModelRenderer { animation: Some(animation.clone()), ..ModelRenderer::new(Vec::new()) };
    let pose = Pose { bones: rest.clone().into() };
    let body = world.spawn((Name("Body".into()), Transform::IDENTITY, Parent(holder), model, pose));
    let camera = world.spawn((Name("Eye".into()), Transform::IDENTITY, Parent(holder), Camera::default()));
    let gun = world.spawn((Name("Gun".into()), Transform::IDENTITY, Parent(holder)));
    if let Some(file) = gun_file {
        world.insert(gun, ModelRenderer { animation: Some(file), ..ModelRenderer::new(Vec::new()) });
        // A spent case to throw out, a cube to look at.
        let look = MeshRenderer::new(MeshHandle(0), MaterialHandle(0));
        world.spawn((Name("Case".into()), Transform::from_position(Vec3::new(0.0, 0.05, 0.12)), Parent(gun), look));
    }
    Player { name, holder, body, camera, gun, animation, rest, frame: 0 }
}

/// Gives the game every clip of a file, as `{name}#clip:{clip}`.
fn put_clips(world: &mut World, name: &str, file: &Arc<ModelAnimation>) {
    let clips = world.resource_mut::<Clips>().expect("the game's clips");
    for (index, clip) in file.clips.iter().enumerate() {
        let source = ClipSource { animation: file.clone(), index, settings: Default::default() };
        clips.put(&format!("{name}#clip:{}", clip.name), source);
    }
}

/// The Own player's hold made into an arms clip, and the clips made from it:
/// its forearm rolled, and its gun lifted, quickly and slowly at the very end,
/// held up for a run, and lifted and brought down as a run's start. Gives what
/// the hold was, with each clip's file by its name.
fn own_into_clips(world: &mut World, own: &Player) -> (Own, Vec<(&'static str, Arc<ModelAnimation>)>) {
    let (arms, palms) = clip_of_hold(world, own);
    let (rolled_file, roll) = rolled(&arms, 40.0);
    // The gun jumps 10 cm against the hands in the last tenth of a second of one clip, and over the last 0.6 s of the
    // other: on the gun, the hands snap back in the first and come back slowly in the second.
    let jump = |over: f32| move |t: f32| ((t - (1.0 - over)) / over).clamp(0.0, 1.0) * 0.1;
    // The run holds the gun 10 cm up; its start lifts it 20 cm and brings it back down to there, over half a second.
    let start = |t: f32| if t < 0.25 { 0.8 * t } else { 0.2 - 0.4 * (t - 0.25) };
    let files = vec![
        ("Arms", arms.clone()),
        ("Rolled", rolled_file),
        ("Snaps", lifted(&arms, 1.0, jump(0.1))),
        ("Slow", lifted(&arms, 1.0, jump(0.6))),
        ("Idle", lifted(&arms, 1.0, |_| 0.0)),
        ("Run", lifted(&arms, 1.0, |_| 0.1)),
        ("Start", lifted(&arms, 0.5, start)),
    ];
    let made = Own {
        shown: gun_from_eye(world, own),
        palms,
        knuckles: [knuckles_on_gun(world, own, 0), knuckles_on_gun(world, own, 1)],
        curled: fingers(world, own, 1),
        ways: finger_ways(world, own, 1),
        roll,
    };
    (made, files)
}

/// An arms clip made from how the gun's own hold has a body: its pose, and a
/// `Carrier` bone under the hips where the gun was, a still clip of a
/// second. Gives the clip's file, with where its palms are on the gun.
fn clip_of_hold(world: &World, own: &Player) -> (Arc<ModelAnimation>, [Vec3; 2]) {
    let body = &own.animation;
    let mut skeleton = body.skeleton.clone();
    let posed = world.get::<Pose>(own.body).unwrap().bones.to_vec();
    let gun = world_matrix(world, own.body).inverse() * world_matrix(world, own.gun);
    let hips = 0;
    let (scale, rotation, translation) = (posed[hips].inverse() * gun).to_scale_rotation_translation();
    skeleton.bones.push(Bone {
        name: "Carrier".into(),
        parent: Some(hips),
        rest: BonePose { translation, rotation, scale },
    });
    let mut frame = PoseEdit::new(&body.skeleton, &posed).local;
    frame.push(BonePose { translation, rotation, scale });
    let frames = 61;
    let clip = Clip { name: "Hold".into(), frames, poses: (0..frames).flat_map(|_| frame.clone()).collect() };
    let palms = [palm_on_gun(world, own, 0), palm_on_gun(world, own, 1)];
    (Arc::new(ModelAnimation { skeleton, skins: Vec::new(), clips: vec![clip] }), palms)
}

/// The clip of `arms` with its right forearm rolled `degrees` about its own
/// length and its hand turned back as it was, so the hand sits and turns
/// just as before: a twist only the forearm shows. Gives the clip's file,
/// and how far its forearm is then rolled from its upper arm.
fn rolled(arms: &Arc<ModelAnimation>, degrees: f32) -> (Arc<ModelAnimation>, f32) {
    let skeleton = &arms.skeleton;
    let arm = Rig::find(skeleton).unwrap().arms[1].clone();
    let mut edit = PoseEdit::new(skeleton, &skeleton.model_matrices(arms.clips[0].frame(0)));
    let (along, hand) = ((edit.position(arm.hand) - edit.position(arm.forearm)).normalize(), edit.rotation(arm.hand));
    edit.turn(arm.forearm, Quat::from_axis_angle(along, degrees.to_radians()));
    edit.set_rotation(arm.hand, hand);
    let roll = forearm_roll(&edit, &arm);
    let frame = edit.local.clone();
    let clip = Clip { name: "Hold".into(), frames: 61, poses: (0..61).flat_map(|_| frame.clone()).collect() };
    (Arc::new(ModelAnimation { skeleton: skeleton.clone(), skins: Vec::new(), clips: vec![clip] }), roll)
}

/// The clip of `arms` remade as `seconds` long, its gun lifted `up(t)`
/// metres `t` seconds in, named Lifted.
fn lifted(arms: &Arc<ModelAnimation>, seconds: f32, up: impl Fn(f32) -> f32) -> Arc<ModelAnimation> {
    let carrier = arms.skeleton.bones.len() - 1;
    let still = arms.clips[0].frame(0).to_vec();
    let frames = (seconds * CLIP_RATE).round() as usize + 1;
    let poses = (0..frames)
        .flat_map(|frame| {
            let mut pose = still.clone();
            pose[carrier].translation.y += up(frame as f32 / CLIP_RATE);
            pose
        })
        .collect();
    let clip = Clip { name: "Lifted".into(), frames, poses };
    Arc::new(ModelAnimation { skeleton: arms.skeleton.clone(), skins: Vec::new(), clips: vec![clip] })
}

/// The pack rifle's file: its skeleton, and its clips: working the bolt, back
/// 8 cm and home over a second, or up 2 cm and down with nothing to throw
/// out; a shot's half second, the trigger turning 40 degrees and back; and
/// the gun at rest.
fn pack_rifle_file() -> Arc<ModelAnimation> {
    let half = |share: f32| (std::f32::consts::PI * share).sin();
    let clips = vec![
        gun_clip("Bolt", 1.0, |share, pose| pose[BOLT].translation.z -= 0.08 * half(share)),
        gun_clip("Bolt Up", 1.0, |share, pose| pose[BOLT].translation.y += 0.02 * half(share)),
        gun_clip("Trigger", 0.5, |share, pose| {
            pose[TRIGGER].rotation = Quat::from_rotation_x(40f32.to_radians() * half(share))
        }),
        gun_clip("Still", 1.0 / 60.0, |_, _| {}),
    ];
    Arc::new(ModelAnimation { skeleton: gun_skeleton(), skins: Vec::new(), clips })
}

fn gun_skeleton() -> Skeleton {
    let at = |translation| BonePose { translation, ..BonePose::default() };
    Skeleton {
        bones: vec![
            Bone { name: "Root".into(), parent: None, rest: at(Vec3::ZERO) },
            Bone { name: "bolt".into(), parent: Some(0), rest: at(Vec3::new(0.0, 0.045, 0.04)) },
            Bone { name: "trigger".into(), parent: Some(0), rest: at(Vec3::new(0.0, -0.017, 0.06)) },
        ],
    }
}

/// A clip of the gun's `seconds` long, each frame's pose from `posed` at its
/// share of the way, 0 to 1.
fn gun_clip(name: &str, seconds: f32, posed: impl Fn(f32, &mut [BonePose])) -> Clip {
    let frames = (seconds * 60.0).round() as usize + 1;
    let rest = gun_skeleton().rest();
    let poses = (0..frames).flat_map(|frame| {
        let mut pose = rest.clone();
        posed(frame as f32 / (frames - 1) as f32, &mut pose);
        pose
    });
    Clip { name: name.into(), frames, poses: poses.collect() }
}

/// What the driver said of a player's gun at a frame of its own.
#[derive(Clone, Debug)]
struct State {
    rounds: u32,
    chamber: String,
    acting: String,
}

/// What the driver said, by the line's kind and player, and the player's
/// frame: each frame's state, and the shots fired and snaps found.
type Said = BTreeMap<(String, usize), String>;

fn said(log: &LogCapture) -> Said {
    let mut said = BTreeMap::new();
    for line in log.lock().unwrap().lines.iter() {
        let words: Vec<&str> = line.message.split(' ').collect();
        match words.as_slice() {
            ["said", name, frame, rest @ ..] => {
                if let Ok(frame) = frame.parse::<usize>() {
                    said.insert((format!("said {name}"), frame), rest.join(" "));
                }
            }
            [kind @ ("fired" | "snaps"), name, value] => {
                said.insert((format!("{kind} {name}"), 0), value.to_string());
            }
            _ => {}
        }
    }
    said
}

fn said_of(said: &Said, player: &Player, frame: usize) -> Option<State> {
    let line = said.get(&(format!("said {}", player.name), frame))?;
    let words: Vec<&str> = line.split(' ').collect();
    let [rounds, chamber, acting] = words.as_slice() else { return None };
    Some(State { rounds: rounds.parse().ok()?, chamber: chamber.to_string(), acting: acting.to_string() })
}

/// How many spent cases lie about: copies of a case, inside no Obj.
fn cases(world: &World) -> usize {
    world
        .query_ref::<(Entity, &Name)>()
        .into_iter()
        .filter(|(entity, name)| name.0 == "Case" && !world.has::<Parent>(*entity))
        .count()
}

/// The pack rifle's bone where its Pose has it, in the gun's space.
fn gun_bone(world: &World, player: &Player, bone: usize) -> Mat4 {
    world.get::<Pose>(player.gun).map_or(Mat4::IDENTITY, |pose| pose.bones[bone])
}

fn gun_at_rest(world: &World, player: &Player) -> Vec<Mat4> {
    let skeleton = &world.get::<ModelRenderer>(player.gun).unwrap().animation.as_ref().unwrap().skeleton;
    skeleton.model_matrices(&skeleton.rest())
}

/// The gun's place in the camera's space.
fn gun_from_eye(world: &World, player: &Player) -> Mat4 {
    world_matrix(world, player.camera).inverse() * world_matrix(world, player.gun)
}

fn posed<'a>(world: &World, player: &'a Player) -> (Rig, PoseEdit<'a>) {
    let skeleton = &player.animation.skeleton;
    let rig = Rig::find(skeleton).unwrap();
    (rig, PoseEdit::new(skeleton, &world.get::<Pose>(player.body).unwrap().bones))
}

/// A place on the body, in the gun's space.
fn on_gun(world: &World, player: &Player, at: Vec3) -> Vec3 {
    let world_at = world_matrix(world, player.body).transform_point3(at);
    world_matrix(world, player.gun).inverse().transform_point3(world_at)
}

/// The middle of a body's palm, left or right, in the gun's space.
fn palm_on_gun(world: &World, player: &Player, arm: usize) -> Vec3 {
    let (rig, edit) = posed(world, player);
    on_gun(world, player, grip::palm_of(&edit, &rig.arms[arm]).unwrap().point)
}

/// The middle of a body's knuckles, left or right, in the gun's space.
fn knuckles_on_gun(world: &World, player: &Player, arm: usize) -> Vec3 {
    let (rig, edit) = posed(world, player);
    on_gun(world, player, grip::knuckles(&edit, &rig.arms[arm]).unwrap())
}

/// How far a body's hand's fingers bend toward its palm, from straight.
fn fingers(world: &World, player: &Player, arm: usize) -> [[f32; 3]; 5] {
    let (rig, edit) = posed(world, player);
    grip::finger_bends(&edit, &rig.arms[arm])
}

/// How far a body's right forearm is rolled from its upper arm, in radians.
fn body_roll(world: &World, player: &Player) -> f32 {
    let (rig, edit) = posed(world, player);
    forearm_roll(&edit, &rig.arms[1])
}

/// How far an arm's forearm is rolled about its own length from how its
/// upper arm holds it, in radians: the twist of its turn from the upper
/// arm's about the line to the wrist.
fn forearm_roll(edit: &PoseEdit, arm: &Arm) -> f32 {
    let turn = edit.rotation(arm.upper).inverse() * edit.rotation(arm.forearm);
    // The same turn either way round, taken the short way, so a roll reads within half a turn.
    let turn = if turn.w < 0.0 { -turn } else { turn };
    let length = edit.local[arm.hand].translation.normalize();
    let along = Vec3::new(turn.x, turn.y, turn.z).dot(length);
    2.0 * along.atan2(turn.w)
}

/// The way each finger joint of a body's hand points, in the hand's frame:
/// to the next joint, or past the last, as the link before it pointed at
/// rest, turned as the joint has turned since.
fn finger_ways(world: &World, player: &Player, arm: usize) -> [[Vec3; 3]; 5] {
    let (rig, edit) = posed(world, player);
    let skeleton = &player.animation.skeleton;
    let rest = PoseEdit::new(skeleton, &skeleton.model_matrices(&skeleton.rest()));
    let arm = &rig.arms[arm];
    let Some((along, palm)) = hand_axes(&edit, arm) else { return [[Vec3::ZERO; 3]; 5] };
    let Some(palm) = (palm - along * palm.dot(along)).try_normalize() else { return [[Vec3::ZERO; 3]; 5] };
    let into = Quat::from_mat3(&Mat3::from_cols(along, palm, along.cross(palm))).inverse();
    let link = |bones: &[usize], joint: usize| -> Option<Vec3> {
        let bone = *bones.get(joint)?;
        if let Some(&next) = bones.get(joint + 1) {
            return (edit.position(next) - edit.position(bone)).try_normalize();
        }
        let before = *bones.get(joint.checked_sub(1)?)?;
        let at_rest = (rest.position(bone) - rest.position(before)).try_normalize()?;
        Some(edit.rotation(bone) * rest.rotation(bone).inverse() * at_rest)
    };
    std::array::from_fn(|finger| {
        std::array::from_fn(|joint| link(&arm.fingers[finger], joint).map_or(Vec3::ZERO, |way| into * way))
    })
}

/// Where a body's camera is from its head, in metres.
fn camera_from_head(world: &World, player: &Player) -> Vec3 {
    let (rig, edit) = posed(world, player);
    let head = world_matrix(world, player.body).transform_point3(edit.position(rig.head));
    world_matrix(world, player.camera).w_axis.truncate() - head
}

/// A body's bone names as `upper_arm.L`, the way the package's own hold's
/// body has them.
fn dotted(part: &str, side: Option<bool>) -> String {
    match side {
        Some(left) => format!("{part}.{}", if left { "L" } else { "R" }),
        None => part.to_string(),
    }
}

/// A body's bone names as `DEF-upper_arm.L`: a body named otherwise.
fn prefixed(part: &str, side: Option<bool>) -> String {
    format!("DEF-{}", dotted(part, side))
}

/// A humanoid standing at the Obj's middle facing +Z, its arms out to the
/// sides, with fingers to close on the gun, its bones named by `name`.
fn body(name: impl Fn(&str, Option<bool>) -> String) -> Skeleton {
    let mut bones: Vec<Bone> = Vec::new();
    let mut add = |part: &str, side: Option<bool>, parent: Option<usize>, at: Vec3| {
        bones.push(Bone { name: name(part, side), parent, rest: BonePose { translation: at, ..BonePose::default() } });
        bones.len() - 1
    };
    let hips = add("hips", None, None, Vec3::new(0.0, 1.0, 0.0));
    let spine = add("spine", None, Some(hips), Vec3::new(0.0, 0.15, 0.0));
    let chest = add("chest", None, Some(spine), Vec3::new(0.0, 0.2, 0.0));
    let neck = add("neck", None, Some(chest), Vec3::new(0.0, 0.2, 0.0));
    add("head", None, Some(neck), Vec3::new(0.0, 0.1, 0.0));
    for left in [true, false] {
        // The engine is left-handed: facing +Z, the left hand is toward -X.
        let out = if left { -1.0 } else { 1.0 };
        let clavicle = add("shoulder", Some(left), Some(chest), Vec3::new(0.05 * out, 0.15, 0.0));
        let upper = add("upper_arm", Some(left), Some(clavicle), Vec3::new(0.15 * out, 0.0, 0.0));
        let twist = add("upper_arm_twist", Some(left), Some(upper), Vec3::new(0.14 * out, 0.0, 0.0));
        let forearm = add("forearm", Some(left), Some(twist), Vec3::new(0.14 * out, 0.0, 0.0));
        let hand = add("hand", Some(left), Some(forearm), Vec3::new(0.26 * out, 0.0, 0.0));
        let index = add("index_01", Some(left), Some(hand), Vec3::new(0.08 * out, 0.0, 0.02));
        add("index_02", Some(left), Some(index), Vec3::new(0.03 * out, 0.0, 0.0));
        add("thumb_01", Some(left), Some(hand), Vec3::new(0.03 * out, -0.02, 0.04));
        add("middle_01", Some(left), Some(hand), Vec3::new(0.085 * out, 0.0, 0.0));
        add("pinky_01", Some(left), Some(hand), Vec3::new(0.075 * out, 0.0, -0.035));
    }
    for left in [true, false] {
        let out = if left { -1.0 } else { 1.0 };
        let thigh = add("thigh", Some(left), Some(hips), Vec3::new(0.1 * out, -0.05, 0.0));
        let shin = add("shin", Some(left), Some(thigh), Vec3::new(0.0, -0.45, 0.0));
        let foot = add("foot", Some(left), Some(shin), Vec3::new(0.0, -0.42, 0.0));
        add("toe", Some(left), Some(foot), Vec3::new(0.0, -0.06, 0.12));
    }
    Skeleton { bones }
}

/// `skeleton` with every bone's axes turned otherwise at rest, each by its
/// own turn, its joints where they were.
fn turned(mut skeleton: Skeleton) -> Skeleton {
    let at: Vec<Vec3> = skeleton.model_matrices(&skeleton.rest()).iter().map(|m| m.w_axis.truncate()).collect();
    let turn = |bone: usize| {
        let bone = bone as f32;
        Quat::from_euler(EulerRot::XYZ, bone * 0.7, bone * 1.3, -0.4)
    };
    for bone in 0..skeleton.bones.len() {
        let parent = skeleton.bones[bone].parent;
        let above = parent.map_or(Quat::IDENTITY, turn);
        skeleton.bones[bone].rest.rotation = above.inverse() * turn(bone);
        skeleton.bones[bone].rest.translation =
            parent.map_or(at[bone], |parent| above.inverse() * (at[bone] - at[parent]));
    }
    skeleton
}

/// The Hello copy in `dir` with the guns package in `package` as a folder
/// package, its driver arming the players, and the Guns module off.
fn with_package(dir: &Path, package: &Path) -> Project {
    let package = package.canonicalize().expect("the guns package's folder");
    let toml = format!("[packages]\nvoxear-guns = {{ path = {:?} }}\n", package.display().to_string());
    std::fs::write(dir.join("packages.toml"), toml).unwrap();
    std::fs::write(dir.join("Scripts/PackDriver.cs"), DRIVER).unwrap();
    let mut project = Project::open(dir).unwrap();
    project.set_module(voxear_engine::modules::Module::Guns, false).unwrap();
    project
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
