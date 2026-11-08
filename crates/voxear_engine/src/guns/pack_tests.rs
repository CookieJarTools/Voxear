//! Guns from animation packs, held: a gun drawn by one skinned model whose
//! bones its own clips move, its actions played from clips part by part.

use std::sync::Arc;

use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_core::time::Time;
use voxear_ecs::{Entity, World};
use voxear_render::{Bone, BonePose, Clip, ModelAnimation, ModelRenderer, Pose, Skeleton};
use voxear_scene::{Parent, Transform};

use super::actions::{ActionKind, ClipPart, Event, Fact, Mark, When};
use super::handling::{ActionOffsets, GunHandling, put_handling};
use super::holder::tests_support::*;
use super::holder::{GunHolder, GunHolderState, fire};
use super::mechanism::{Chamber, GunPart, GunState, PartRole};
use super::{Gun, GunKind};
use crate::animation::{ClipSource, Clips};

/// The gun's bones: its root, its bolt and its trigger.
const BOLT: usize = 1;
const TRIGGER: usize = 2;

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

/// A clip of the gun's `seconds` long, each frame's pose from `posed` at
/// its share of the way, 0 to 1.
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

/// A bolt-action rifle drawn by one skinned model with the gun's clips in
/// its own file: working the bolt, back 8 cm and home over a second, or up
/// 2 cm and down with nothing to throw out; a shot's half second, the
/// trigger turning 40 degrees and back; and the gun at rest. The handling
/// works the bolt from clips, the case thrown out and the next round
/// chambered at marks, and plays the trigger's clip at each shot.
fn pack_rifle() -> (Scene, Arc<ModelAnimation>) {
    let mut scene = scene_with(GunKind::BoltActionRifle);
    let half = |share: f32| (std::f32::consts::PI * share).sin();
    let clips = vec![
        gun_clip("Hammer", 1.0, |share, pose| pose[BOLT].translation.z -= 0.08 * half(share)),
        gun_clip("Hammer_No_Bullet", 1.0, |share, pose| pose[BOLT].translation.y += 0.02 * half(share)),
        gun_clip("Fire", 0.5, |share, pose| {
            pose[TRIGGER].rotation = Quat::from_rotation_x(40f32.to_radians() * half(share))
        }),
        gun_clip("Rest", 1.0 / 60.0, |_, _| {}),
    ];
    let animation = Arc::new(ModelAnimation { skeleton: gun_skeleton(), skins: Vec::new(), clips });
    let model = ModelRenderer { animation: Some(animation.clone()), ..ModelRenderer::new(Vec::new()) };
    scene.world.insert(scene.gun, model);
    let mut found = Clips::default();
    for (index, clip) in animation.clips.iter().enumerate() {
        let source = ClipSource { animation: animation.clone(), index, settings: Default::default() };
        found.put(&format!("Gun#clip:{}", clip.name), source);
    }
    scene.world.insert_resource(found);
    // A spent case to throw out, a cube to look at.
    let look = voxear_render::MeshRenderer::new(voxear_render::MeshHandle(0), voxear_render::MaterialHandle(0));
    let case = GunPart::of(PartRole::Case, [0.0, 0.05, 0.12]);
    scene.world.spawn((Transform::from_position(Vec3::new(0.0, 0.05, 0.12)), Parent(scene.gun), case, look));

    let mark = |at, event| Mark { at, event };
    let throws = ClipPart {
        gun: "Gun#clip:Hammer".into(),
        marks: vec![mark(0.5, Event::Eject), mark(0.8, Event::Chamber)],
        when: When::all([Fact::ChamberFilled]),
        ..ClipPart::default()
    };
    let empty = ClipPart {
        gun: "Gun#clip:Hammer_No_Bullet".into(),
        marks: vec![mark(0.8, Event::Chamber)],
        when: When::all([Fact::ChamberEmpty]),
        ..ClipPart::default()
    };
    let mut handling = GunHandling::default();
    handling.offsets.push(ActionOffsets {
        action: ActionKind::Cycle,
        parts: vec![throws, empty],
        ..ActionOffsets::default()
    });
    handling.pack.rest = "Gun#clip:Rest".into();
    handling.pack.gun_fire = "Gun#clip:Fire".into();
    put_handling(&mut scene.world, "Pack", Arc::new(handling));
    scene.world.get_mut::<Gun>(scene.gun).unwrap().handling = "Pack".into();
    (scene, animation)
}

fn step(scene: &mut Scene, rest: &[Mat4], time: &mut Time) {
    time.elapsed += f64::from(time.delta);
    scene.world.insert_resource(*time);
    frame(scene, rest);
}

/// The gun's bone where its Pose has it, in the gun's space.
fn bone(world: &World, gun: Entity, bone: usize) -> Mat4 {
    world.get::<Pose>(gun).unwrap().bones[bone]
}

fn cases(world: &World) -> usize {
    let leftovers = world.query_ref::<(&voxear_scene::Name, &super::leftovers::Leftover)>().into_iter();
    leftovers.filter(|(name, _)| name.0 == "Spent Case").count()
}

#[test]
fn a_shot_plays_the_guns_fire_clip_then_its_bolt_is_worked_from_clips_marks_and_all() {
    let (mut scene, animation) = pack_rifle();
    let rest = rest_of(&scene);
    let at_rest = animation.skeleton.model_matrices(&animation.skeleton.rest());
    let mut time = Time { delta: 1.0 / 60.0, ..Time::default() };
    for _ in 0..5 {
        step(&mut scene, &rest, &mut time);
    }
    assert_eq!(bone(&scene.world, scene.gun, BOLT), at_rest[BOLT], "standing in its rest clip");
    assert!(fire(&mut scene.world, scene.player).is_some());

    // A quarter of a second on, the trigger is pulled all the way, as the fire clip has it.
    for _ in 0..15 {
        step(&mut scene, &rest, &mut time);
    }
    let pulled = bone(&scene.world, scene.gun, TRIGGER).to_scale_rotation_translation().1;
    assert!((pulled.angle_between(Quat::IDENTITY).to_degrees() - 40.0).abs() < 1.5, "{pulled}");
    let state = scene.world.get::<GunHolderState>(scene.player).unwrap();
    assert_eq!(state.acting(), None, "the bolt waits for the fire clip to end");

    // Then the bolt is worked: the case has something to throw out, so the clip that throws it plays.
    let mut furthest: f32 = 0.0;
    let (mut thrown_at, mut chambered_at) = (None, None);
    for frame in 0..100 {
        step(&mut scene, &rest, &mut time);
        let back = at_rest[BOLT].w_axis.z - bone(&scene.world, scene.gun, BOLT).w_axis.z;
        furthest = furthest.max(back);
        let gun = scene.world.get::<GunState>(scene.gun).unwrap();
        if thrown_at.is_none() && cases(&scene.world) == 1 {
            thrown_at = Some(frame);
        }
        if chambered_at.is_none() && gun.chamber == Chamber::Live {
            chambered_at = Some(frame);
        }
    }
    assert!((furthest - 0.08).abs() < 0.002, "the bolt went back as the clip takes it: {furthest}");
    let (thrown, chambered) = (thrown_at.unwrap(), chambered_at.unwrap());
    assert!((chambered as i32 - thrown as i32 - 18).abs() <= 1, "0.3 s apart, as marked: {thrown}, {chambered}");
    assert_eq!(cases(&scene.world), 1, "one case, thrown once");
    let gun = scene.world.get::<GunState>(scene.gun).unwrap();
    assert!(gun.ready(GunKind::BoltActionRifle) && gun.rounds == 4, "{gun:?}");
    assert_eq!(bone(&scene.world, scene.gun, BOLT), at_rest[BOLT], "and back in its rest clip");
}

#[test]
fn working_the_bolt_on_an_empty_chamber_plays_the_clip_that_throws_nothing() {
    let (mut scene, animation) = pack_rifle();
    let rest = rest_of(&scene);
    let at_rest = animation.skeleton.model_matrices(&animation.skeleton.rest());
    let mut time = Time { delta: 1.0 / 60.0, ..Time::default() };
    step(&mut scene, &rest, &mut time);
    scene.world.get_mut::<GunState>(scene.gun).unwrap().chamber = Chamber::Empty;
    scene.world.get_mut::<GunHolder>(scene.player).unwrap().request = Some(ActionKind::Cycle.into());
    let mut highest: f32 = 0.0;
    for _ in 0..30 {
        step(&mut scene, &rest, &mut time);
        highest = highest.max(bone(&scene.world, scene.gun, BOLT).w_axis.y - at_rest[BOLT].w_axis.y);
    }
    assert!(highest > 0.015, "the bolt went up, as the no-round clip takes it: {highest}");
    assert_eq!(cases(&scene.world), 0);
    for _ in 0..40 {
        step(&mut scene, &rest, &mut time);
    }
    let gun = scene.world.get::<GunState>(scene.gun).unwrap();
    assert!(gun.chamber == Chamber::Live && gun.rounds == 4, "chambered at its mark: {gun:?}");
}

/// Holds still: no sway, bob or breath, so a hold can be compared.
fn steady(scene: &mut Scene) {
    let holder = &mut scene.world.get_mut::<GunHolder>(scene.player).unwrap();
    (holder.sway, holder.bob, holder.breathing, holder.wall_check) = (0.0, 0.0, 0.0, false);
}

/// The middle of a body's palm, left or right, in the gun's space.
fn palm_on_gun(scene: &Scene, arm: usize) -> Vec3 {
    use crate::animation::ik::PoseEdit;
    use crate::animation::rig::Rig;
    let animation = scene.world.get::<ModelRenderer>(scene.body).unwrap().animation.clone().unwrap();
    let rig = Rig::find(&animation.skeleton).unwrap();
    let edit = PoseEdit::new(&animation.skeleton, &scene.world.get::<Pose>(scene.body).unwrap().bones);
    let palm = super::hands::palm_of(&edit, &rig.arms[arm]).unwrap().point;
    let world = voxear_scene::world_matrix(&scene.world, scene.body).transform_point3(palm);
    voxear_scene::world_matrix(&scene.world, scene.gun).inverse().transform_point3(world)
}

/// How far a body's hand's fingers bend toward its palm, from straight.
fn fingers(scene: &Scene, arm: usize) -> [[f32; 3]; 5] {
    use crate::animation::ik::PoseEdit;
    use crate::animation::rig::Rig;
    let animation = scene.world.get::<ModelRenderer>(scene.body).unwrap().animation.clone().unwrap();
    let rig = Rig::find(&animation.skeleton).unwrap();
    let edit = PoseEdit::new(&animation.skeleton, &scene.world.get::<Pose>(scene.body).unwrap().bones);
    super::hands::finger_bends(&edit, &rig.arms[arm])
}

/// The gun's place in the camera's space.
fn gun_from_eye(scene: &Scene) -> Mat4 {
    voxear_scene::world_matrix(&scene.world, scene.camera).inverse()
        * voxear_scene::world_matrix(&scene.world, scene.gun)
}

/// An arms clip made from how the gun's own hold has a body: its pose, and
/// a `gun_carrier` bone under the hips where the gun was, a still clip of a
/// second. Gives the clip's file, with where its palms are on the gun.
fn clip_of_hold(scene: &mut Scene) -> (Arc<ModelAnimation>, [Vec3; 2]) {
    use crate::animation::ik::PoseEdit;
    let body = scene.world.get::<ModelRenderer>(scene.body).unwrap().animation.clone().unwrap();
    let mut skeleton = body.skeleton.clone();
    let posed = scene.world.get::<Pose>(scene.body).unwrap().bones.to_vec();
    let gun = voxear_scene::world_matrix(&scene.world, scene.body).inverse()
        * voxear_scene::world_matrix(&scene.world, scene.gun);
    let hips = 0;
    let (scale, rotation, translation) = (posed[hips].inverse() * gun).to_scale_rotation_translation();
    skeleton.bones.push(Bone {
        name: "gun_carrier".into(),
        parent: Some(hips),
        rest: BonePose { translation, rotation, scale },
    });
    let mut frame = PoseEdit::new(&body.skeleton, &posed).local;
    frame.push(BonePose { translation, rotation, scale });
    let frames = 61;
    let clip = Clip { name: "Hold".into(), frames, poses: (0..frames).flat_map(|_| frame.clone()).collect() };
    let palms = [palm_on_gun(scene, 0), palm_on_gun(scene, 1)];
    (Arc::new(ModelAnimation { skeleton, skins: Vec::new(), clips: vec![clip] }), palms)
}

/// A player as `scene_with` makes one, but with the body `skeleton`.
fn scene_with_body(skeleton: Skeleton) -> Scene {
    let mut scene = scene_with(GunKind::BoltActionRifle);
    let rest = skeleton.model_matrices(&skeleton.rest());
    let animation = Arc::new(ModelAnimation { skeleton, skins: Vec::new(), clips: Vec::new() });
    scene.world.insert(scene.body, ModelRenderer { animation: Some(animation), ..ModelRenderer::new(Vec::new()) });
    scene.world.insert(scene.body, Pose { bones: rest.into() });
    scene
}

/// Holds the gun by the arms clip `arms` alone.
fn held_by(scene: &mut Scene, arms: &Arc<ModelAnimation>) {
    let mut clips = Clips::default();
    clips.put("Arms#clip:Hold", ClipSource { animation: arms.clone(), index: 0, settings: Default::default() });
    scene.world.insert_resource(clips);
    let mut handling = GunHandling::default();
    handling.pack.idle = "Arms#clip:Hold".into();
    // The clip was made with the holder's own eyes, which sit up and forward of its head.
    handling.pack.eyes = crate::first_person::FirstPersonView::default().eyes;
    put_handling(&mut scene.world, "Held", Arc::new(handling));
    scene.world.get_mut::<Gun>(scene.gun).unwrap().handling = "Held".into();
}

fn settle(scene: &mut Scene, frames: usize) {
    let rest = rest_of(scene);
    let mut time = Time { delta: 1.0 / 60.0, ..Time::default() };
    for _ in 0..frames {
        step(scene, &rest, &mut time);
    }
}

#[test]
fn an_arms_clip_holds_the_gun_where_it_has_it_and_puts_the_palms_where_its_are() {
    // The gun's own hold, made into a clip.
    let mut own = scene_with(GunKind::BoltActionRifle);
    steady(&mut own);
    settle(&mut own, 40);
    let (arms, palms) = clip_of_hold(&mut own);
    let shown = gun_from_eye(&own);
    let curled = fingers(&own, 1);

    // Held by the clip alone, on the same body: the gun where the clip has it, and each palm where its was.
    let mut scene = scene_with(GunKind::BoltActionRifle);
    steady(&mut scene);
    held_by(&mut scene, &arms);
    settle(&mut scene, 40);
    let held = gun_from_eye(&scene);
    let off = held.w_axis.truncate().distance(shown.w_axis.truncate());
    assert!(off < 0.003, "the gun where the clip carries it: {off} m off");
    for arm in 0..2 {
        let off = palm_on_gun(&scene, arm).distance(palms[arm]);
        assert!(off < 0.005, "palm {arm} on the gun where the clip's is: {off} m off");
    }
    let (index, clip_index) = (fingers(&scene, 1)[1], curled[1]);
    assert!(clip_index[0] > 0.3, "the clip's trigger finger is bent: {clip_index:?}");
    for joint in 0..2 {
        let off = (index[joint] - clip_index[joint]).abs().to_degrees();
        assert!(off < 3.0, "the index finger bends as the clip's, joint {joint}: {off} degrees off");
    }

    // On a body named otherwise and a tenth shorter, the palms go where the clip's are on the gun all the same:
    // the shooting hand's exactly, the gun coming nearer for it, and the other's slid back along the gun as far
    // as its shorter arm needs.
    let mut shorter = crate::animation::rig::tests::body(|part, side| match side {
        Some(left) => format!("DEF-{part}.{}", if left { "L" } else { "R" }),
        None => format!("DEF-{part}"),
    });
    shorter.bones.iter_mut().for_each(|bone| bone.rest.translation *= 0.9);
    let mut other = scene_with_body(shorter);
    steady(&mut other);
    held_by(&mut other, &arms);
    settle(&mut other, 40);
    let off = palm_on_gun(&other, 1).distance(palms[1]);
    assert!(off < 0.005, "on the shorter body, the shooting palm: {off} m off");
    let moved = palm_on_gun(&other, 0) - palms[0];
    let aside = Vec3::new(moved.x, moved.y, 0.0).length();
    assert!(aside < 0.005 && moved.z <= 0.003 && moved.z > -0.1, "the support palm, slid back along the gun: {moved}");

    // Aimed, and shot, the gun moves, and the hands stay on it.
    scene.world.get_mut::<GunHolder>(scene.player).unwrap().aiming = true;
    settle(&mut scene, 30);
    assert!(gun_from_eye(&scene).w_axis.truncate().distance(held.w_axis.truncate()) > 0.05, "up to the eye");
    assert!(fire(&mut scene.world, scene.player).is_some());
    settle(&mut scene, 3);
    for arm in 0..2 {
        let off = palm_on_gun(&scene, arm).distance(palms[arm]);
        assert!(off < 0.006, "aimed and kicking, palm {arm} stays on the gun: {off} m off");
    }
}

#[test]
fn holding_clips_share_the_hold_by_how_fast_the_holder_goes() {
    use super::clip_hold::holding_weights;
    let mut pack =
        super::handling::PackClips { idle: "I".into(), walk: "W".into(), run: "R".into(), ..Default::default() };
    (pack.walk_speed, pack.run_speed) = (1.5, 4.0);
    let near = |a: [f32; 4], b: [f32; 4]| a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-5);
    assert!(near(holding_weights(&pack, 0.0, false), [1.0, 0.0, 0.0, 0.0]));
    assert!(near(holding_weights(&pack, 0.75, false), [0.5, 0.5, 0.0, 0.0]));
    assert!(near(holding_weights(&pack, 2.75, false), [0.0, 0.5, 0.5, 0.0]));
    assert!(near(holding_weights(&pack, 9.0, false), [0.0, 0.0, 1.0, 0.0]));
    assert!(near(holding_weights(&pack, 9.0, true), [0.0, 0.0, 1.0, 0.0]), "no sprint clip: running");
    pack.sprint = "S".into();
    assert!(near(holding_weights(&pack, 0.0, true), [0.0, 0.0, 0.0, 1.0]));
    pack.run.clear();
    assert!(near(holding_weights(&pack, 9.0, false), [0.0, 1.0, 0.0, 0.0]), "no run clip: walking");
}
