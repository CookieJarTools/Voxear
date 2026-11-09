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
        gun_clip("Cycle", 1.0, |share, pose| pose[BOLT].translation.z -= 0.08 * half(share)),
        gun_clip("Cycle_Empty", 1.0, |share, pose| pose[BOLT].translation.y += 0.02 * half(share)),
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
        gun: "Gun#clip:Cycle".into(),
        marks: vec![mark(0.5, Event::Eject), mark(0.8, Event::Chamber)],
        when: When::all([Fact::ChamberFilled]),
        ..ClipPart::default()
    };
    let empty = ClipPart {
        gun: "Gun#clip:Cycle_Empty".into(),
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
    let acting = scene.world.get::<GunHolderState>(scene.player).unwrap().acting().is_some();
    let view = scene.world.get::<crate::first_person::FirstPersonView>(scene.player).unwrap().rotation();
    let looks = voxear_scene::world_matrix(&scene.world, scene.camera).to_scale_rotation_translation().1;
    assert!(acting && looks.angle_between(view) < 1e-3, "the camera looks where the view does, as the clips' did");
    assert_eq!(cases(&scene.world), 0);
    for _ in 0..40 {
        step(&mut scene, &rest, &mut time);
    }
    let gun = scene.world.get::<GunState>(scene.gun).unwrap();
    assert!(gun.chamber == Chamber::Live && gun.rounds == 4, "chambered at its mark: {gun:?}");
}

/// Holds still: no sway, bob or breath, so a hold can be compared; and with
/// the head shown, so a hold made into a clip has its head where it is.
fn steady(scene: &mut Scene) {
    let holder = &mut scene.world.get_mut::<GunHolder>(scene.player).unwrap();
    (holder.sway, holder.bob, holder.breathing, holder.wall_check) = (0.0, 0.0, 0.0, false);
    scene.world.get_mut::<crate::first_person::FirstPersonView>(scene.player).unwrap().hide_head = false;
}

/// The middle of a body's palm, left or right, in the gun's space.
fn palm_on_gun(scene: &Scene, arm: usize) -> Vec3 {
    use crate::animation::ik::PoseEdit;
    use crate::animation::rig::Rig;
    let animation = scene.world.get::<ModelRenderer>(scene.body).unwrap().animation.clone().unwrap();
    let rig = Rig::find(&animation.skeleton).unwrap();
    let edit = PoseEdit::new(&animation.skeleton, &scene.world.get::<Pose>(scene.body).unwrap().bones);
    let palm = crate::animation::grip::palm_of(&edit, &rig.arms[arm]).unwrap().point;
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
    crate::animation::grip::finger_bends(&edit, &rig.arms[arm])
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
    held_from(scene, arms, super::handling::PackCamera::Filmed);
}

/// Holds the gun by the arms clip `arms` alone, looking from `camera`.
fn held_from(scene: &mut Scene, arms: &Arc<ModelAnimation>, camera: super::handling::PackCamera) {
    let mut clips = Clips::default();
    clips.put("Arms#clip:Hold", ClipSource { animation: arms.clone(), index: 0, settings: Default::default() });
    scene.world.insert_resource(clips);
    let mut handling = GunHandling::default();
    handling.pack.idle = "Arms#clip:Hold".into();
    // The clip was made with the holder's own eyes, which sit up and forward of its head.
    handling.pack.eyes = crate::first_person::FirstPersonView::default().eyes;
    handling.pack.camera = camera;
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

    // On a body named otherwise and a tenth shorter, both palms stay where the clip's are on the gun, which shows
    // just where the clip shows it but a tenth nearer: the camera comes to where the clip's was from the shorter
    // body's shoulders, and the hold as much nearer the eyes, so the arms reach it as bent as the clip's; and so they
    // do with the view tipped well down or up.
    let mut shorter = crate::animation::rig::tests::body(|part, side| match side {
        Some(left) => format!("DEF-{part}.{}", if left { "L" } else { "R" }),
        None => format!("DEF-{part}"),
    });
    shorter.bones.iter_mut().for_each(|bone| bone.rest.translation *= 0.9);
    let mut other = scene_with_body(shorter);
    steady(&mut other);
    held_by(&mut other, &arms);
    for pitch in [0.0, -55.0, 50.0] {
        other.world.get_mut::<crate::first_person::FirstPersonView>(other.player).unwrap().pitch = pitch;
        settle(&mut other, 40);
        for arm in 0..2 {
            let off = palm_on_gun(&other, arm).distance(palms[arm]);
            assert!(off < 0.005, "on the shorter body, looking {pitch} degrees up, palm {arm}: {off} m off");
        }
        if pitch == 0.0 {
            let off = gun_from_eye(&other).w_axis.truncate().distance(shown.w_axis.truncate() * 0.9);
            assert!(off < 0.003, "the gun where the clip shows it, a tenth nearer: {off} m off");
        }
    }

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
fn aimed_a_gun_held_by_clips_comes_up_no_nearer_than_they_hold_it() {
    let mut own = scene_with(GunKind::BoltActionRifle);
    steady(&mut own);
    settle(&mut own, 40);
    let (arms, _) = clip_of_hold(&mut own);
    let mut scene = scene_with(GunKind::BoltActionRifle);
    steady(&mut scene);
    held_by(&mut scene, &arms);
    settle(&mut scene, 40);
    let rear = Vec3::from(scene.world.get::<Gun>(scene.gun).unwrap().rear_sight);
    let held = gun_from_eye(&scene).transform_point3(rear).z;

    // Its eye relief nearer than the clips hold it: the sights come onto the camera's line as far out as the clips
    // have them, the hands no nearer the eye.
    scene.world.get_mut::<Gun>(scene.gun).unwrap().eye_relief = held - 0.15;
    scene.world.get_mut::<GunHolder>(scene.player).unwrap().aiming = true;
    settle(&mut scene, 60);
    let sighted = gun_from_eye(&scene).transform_point3(rear);
    assert!(sighted.truncate().length() < 0.003, "the rear sight on the camera's line: {sighted}");
    assert!((sighted.z - held).abs() < 0.005, "as far out as the clips hold it, {held} m: {sighted}");

    // Further than they hold it: as far as its eye relief says.
    scene.world.get_mut::<Gun>(scene.gun).unwrap().eye_relief = held + 0.1;
    settle(&mut scene, 60);
    let sighted = gun_from_eye(&scene).transform_point3(rear);
    assert!((sighted.z - (held + 0.1)).abs() < 0.005, "at the eye relief: {sighted}");
}

#[test]
fn a_gun_held_by_clips_neither_bobs_nor_breathes_as_its_clips_do_that_themselves() {
    let mut own = scene_with(GunKind::BoltActionRifle);
    steady(&mut own);
    settle(&mut own, 40);
    let (arms, _) = clip_of_hold(&mut own);

    // Bob and breathing on, as a holder has them; sway off, so walking on does not lag the gun.
    let mut scene = scene_with(GunKind::BoltActionRifle);
    steady(&mut scene);
    let holder = scene.world.get_mut::<GunHolder>(scene.player).unwrap();
    (holder.bob, holder.breathing) = (1.0, 1.0);
    held_by(&mut scene, &arms);
    settle(&mut scene, 40);
    let still = gun_from_eye(&scene).w_axis.truncate();

    // Two seconds walking at 3 metres a second: a footfall every 0.75 m, and breathing all along.
    let rest = rest_of(&scene);
    let mut time = Time { delta: 1.0 / 60.0, ..Time::default() };
    let mut most: f32 = 0.0;
    for _ in 0..120 {
        scene.world.get_mut::<Transform>(scene.player).unwrap().position.z += 3.0 / 60.0;
        step(&mut scene, &rest, &mut time);
        most = most.max(gun_from_eye(&scene).w_axis.truncate().distance(still));
    }
    assert!(most < 0.0005, "the gun stays where the clip has it before the eyes: {most} m off");
}

/// `skeleton` with every bone's axes turned otherwise at rest, each by its
/// own turn, its joints where they were.
fn turned(mut skeleton: Skeleton) -> Skeleton {
    let at: Vec<Vec3> = skeleton.model_matrices(&skeleton.rest()).iter().map(|m| m.w_axis.truncate()).collect();
    let turn = |bone: usize| {
        let bone = bone as f32;
        Quat::from_euler(voxear_core::math::EulerRot::XYZ, bone * 0.7, bone * 1.3, -0.4)
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

/// The clip of `arms` with its right forearm rolled `degrees` about its own
/// length and its hand turned back as it was, so the hand sits and turns
/// just as before: a twist only the forearm shows. Gives the clip's file,
/// and how far its forearm is then rolled from its upper arm.
fn rolled(arms: &Arc<ModelAnimation>, degrees: f32) -> (Arc<ModelAnimation>, f32) {
    use crate::animation::ik::PoseEdit;
    use crate::animation::rig::Rig;
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

/// How far an arm's forearm is rolled about its own length from how its
/// upper arm holds it, in radians: the twist of its turn from the upper
/// arm's about the line to the wrist.
fn forearm_roll(edit: &crate::animation::ik::PoseEdit, arm: &crate::animation::rig::Arm) -> f32 {
    let turn = edit.rotation(arm.upper).inverse() * edit.rotation(arm.forearm);
    // The same turn either way round, taken the short way, so a roll reads within half a turn.
    let turn = if turn.w < 0.0 { -turn } else { turn };
    let length = edit.local[arm.hand].translation.normalize();
    let along = Vec3::new(turn.x, turn.y, turn.z).dot(length);
    2.0 * along.atan2(turn.w)
}

/// How far a body's right forearm is rolled from its upper arm, in radians.
fn body_roll(scene: &Scene) -> f32 {
    use crate::animation::ik::PoseEdit;
    use crate::animation::rig::Rig;
    let animation = scene.world.get::<ModelRenderer>(scene.body).unwrap().animation.clone().unwrap();
    let arm = Rig::find(&animation.skeleton).unwrap().arms[1].clone();
    let edit = PoseEdit::new(&animation.skeleton, &scene.world.get::<Pose>(scene.body).unwrap().bones);
    forearm_roll(&edit, &arm)
}

#[test]
fn an_arm_the_clips_hold_turns_as_theirs_does_twist_and_all() {
    let mut own = scene_with(GunKind::BoltActionRifle);
    steady(&mut own);
    settle(&mut own, 40);
    let (arms, palms) = clip_of_hold(&mut own);
    let (arms, roll) = rolled(&arms, 40.0);
    assert!(roll.to_degrees().abs() > 30.0, "the clip's forearm is rolled: {} degrees", roll.to_degrees());

    // The forearm rolls as the clip's, on the same body and on one a tenth shorter named otherwise, a twist no
    // place of the hand shows; and both palms still hold where the clip's do.
    let mut shorter = crate::animation::rig::tests::body(|part, side| match side {
        Some(left) => format!("DEF-{part}.{}", if left { "L" } else { "R" }),
        None => format!("DEF-{part}"),
    });
    shorter.bones.iter_mut().for_each(|bone| bone.rest.translation *= 0.9);
    let same = own.world.get::<ModelRenderer>(own.body).unwrap().animation.clone().unwrap().skeleton.clone();
    for (name, body) in [("the same body", same), ("the shorter body", shorter)] {
        let mut scene = scene_with_body(body);
        steady(&mut scene);
        held_by(&mut scene, &arms);
        settle(&mut scene, 40);
        let off = (body_roll(&scene) - roll).to_degrees().abs();
        assert!(off < 3.0, "on {name}, the forearm rolls as the clip's: {off} degrees off");
        for arm in 0..2 {
            let off = palm_on_gun(&scene, arm).distance(palms[arm]);
            assert!(off < 0.005, "on {name}, palm {arm} where the clip's is: {off} m off");
        }
    }
}

/// The way each finger joint of a body's hand points, in the hand's frame.
fn finger_ways(scene: &Scene, arm: usize) -> [[Vec3; 3]; 5] {
    use crate::animation::ik::PoseEdit;
    use crate::animation::rig::Rig;
    let animation = scene.world.get::<ModelRenderer>(scene.body).unwrap().animation.clone().unwrap();
    let skeleton = &animation.skeleton;
    let rig = Rig::find(skeleton).unwrap();
    let edit = PoseEdit::new(skeleton, &scene.world.get::<Pose>(scene.body).unwrap().bones);
    let rest = PoseEdit::new(skeleton, &skeleton.model_matrices(&skeleton.rest()));
    crate::animation::grip::finger_ways(&edit, &rest, &rig.arms[arm])
}

/// Where a body's camera is from its head, in metres.
fn camera_from_head(scene: &Scene) -> Vec3 {
    use crate::animation::rig::Rig;
    let animation = scene.world.get::<ModelRenderer>(scene.body).unwrap().animation.clone().unwrap();
    let head = Rig::find(&animation.skeleton).unwrap().head;
    let head = scene.world.get::<Pose>(scene.body).unwrap().bones[head].w_axis.truncate();
    let head = voxear_scene::world_matrix(&scene.world, scene.body).transform_point3(head);
    voxear_scene::world_matrix(&scene.world, scene.camera).w_axis.truncate() - head
}

#[test]
fn seen_as_filmed_a_body_built_otherwise_holds_the_gun_as_the_clip_does_fingers_and_all() {
    let mut own = scene_with(GunKind::BoltActionRifle);
    steady(&mut own);
    settle(&mut own, 40);
    let (arms, palms) = clip_of_hold(&mut own);
    let shown = gun_from_eye(&own).w_axis.truncate();
    let ways = finger_ways(&own, 1);

    // A tenth shorter, named otherwise, and every bone's axes turned otherwise at rest.
    let body = crate::animation::rig::tests::body(|part, side| match side {
        Some(left) => format!("DEF-{part}.{}", if left { "L" } else { "R" }),
        None => format!("DEF-{part}"),
    });
    let mut body = turned(body);
    body.bones.iter_mut().for_each(|bone| bone.rest.translation *= 0.9);

    // Seen as filmed, the camera comes to where the clip's was from the shoulders: the gun shows near where it
    // shows in the clip, both hands hold where the clip's do, and the fingers point as the clip's.
    let mut filmed = scene_with_body(body.clone());
    steady(&mut filmed);
    held_by(&mut filmed, &arms);
    settle(&mut filmed, 60);
    for arm in 0..2 {
        let off = palm_on_gun(&filmed, arm).distance(palms[arm]);
        assert!(off < 0.005, "seen as filmed, palm {arm}: {off} m off");
    }
    let off = gun_from_eye(&filmed).w_axis.truncate().distance(shown);
    assert!(off < 0.04, "the gun shows near where the clip shows it: {off} m off");
    let held = finger_ways(&filmed, 1);
    for joint in 0..2 {
        let off = held[1][joint].angle_between(ways[1][joint]).to_degrees();
        assert!(off < 3.0, "the index finger's joint {joint} points as the clip's: {off} degrees off");
    }
    let moved = camera_from_head(&filmed);

    // At the eyes, the camera stays at the head where the view has it, the gun where the clip shows it from there,
    // and the arms reach for it as far as they go: higher above the shoulders than the clip's camera, the eyes put
    // it a little out of the support hand's reach, which stops short rather than move the gun.
    let mut eyes = scene_with_body(body);
    steady(&mut eyes);
    held_from(&mut eyes, &arms, super::handling::PackCamera::Eyes);
    settle(&mut eyes, 60);
    let at_eyes = camera_from_head(&eyes);
    let view = Vec3::from(crate::first_person::FirstPersonView::default().eyes);
    assert!(at_eyes.distance(view) < 0.01, "at the eyes: {at_eyes} {view}");
    assert!(moved.distance(view) > 0.01, "as filmed, it moved: {moved} {view}");
    let off = gun_from_eye(&eyes).w_axis.truncate().distance(shown);
    assert!(off < 0.003, "at the eyes, the gun where the clip shows it: {off} m off");
    for (arm, most) in [(0, 0.02), (1, 0.005)] {
        let off = palm_on_gun(&eyes, arm).distance(palms[arm]);
        assert!(off < most, "at the eyes, palm {arm}: {off} m off");
    }
}

/// The middle of a body's knuckles, left or right, in the gun's space.
fn knuckles_on_gun(scene: &Scene, arm: usize) -> Vec3 {
    use crate::animation::ik::PoseEdit;
    use crate::animation::rig::Rig;
    let animation = scene.world.get::<ModelRenderer>(scene.body).unwrap().animation.clone().unwrap();
    let rig = Rig::find(&animation.skeleton).unwrap();
    let edit = PoseEdit::new(&animation.skeleton, &scene.world.get::<Pose>(scene.body).unwrap().bones);
    let knuckles = crate::animation::grip::knuckles(&edit, &rig.arms[arm]).unwrap();
    let world = voxear_scene::world_matrix(&scene.world, scene.body).transform_point3(knuckles);
    voxear_scene::world_matrix(&scene.world, scene.gun).inverse().transform_point3(world)
}

#[test]
fn hands_with_longer_palms_close_where_the_clips_do_and_hold_on_through_a_fast_turn() {
    let mut own = scene_with(GunKind::BoltActionRifle);
    steady(&mut own);
    settle(&mut own, 40);
    let (arms, _) = clip_of_hold(&mut own);
    let knuckles = [knuckles_on_gun(&own, 0), knuckles_on_gun(&own, 1)];

    // A tenth shorter, but with palms a quarter longer: the fingers start further from the wrist.
    let mut body = crate::animation::rig::tests::body(|part, side| match side {
        Some(left) => format!("DEF-{part}.{}", if left { "L" } else { "R" }),
        None => format!("DEF-{part}"),
    });
    body.bones.iter_mut().for_each(|bone| bone.rest.translation *= 0.9);
    let hands: Vec<usize> = (0..body.bones.len()).filter(|&bone| body.bones[bone].name.contains("hand")).collect();
    for bone in body.bones.iter_mut().filter(|bone| bone.parent.is_some_and(|parent| hands.contains(&parent))) {
        bone.rest.translation *= 1.25;
    }
    let mut scene = scene_with_body(body);
    steady(&mut scene);
    held_by(&mut scene, &arms);
    settle(&mut scene, 40);
    // Swaying as a held gun does, turning fast: the gun lags the turn, and the arms still hold it as the clip does.
    scene.world.get_mut::<GunHolder>(scene.player).unwrap().sway = 1.0;
    let rest = rest_of(&scene);
    let mut time = Time { delta: 1.0 / 60.0, ..Time::default() };
    let mut worst: f32 = 0.0;
    for frame in 0..60 {
        let turn = if frame < 30 { 4.0 } else { -4.0 };
        scene.world.get_mut::<crate::first_person::FirstPersonView>(scene.player).unwrap().yaw += turn;
        step(&mut scene, &rest, &mut time);
        for arm in 0..2 {
            worst = worst.max(knuckles_on_gun(&scene, arm).distance(knuckles[arm]));
        }
    }
    assert!(
        worst < 0.006,
        "the knuckles where the clip's are on the gun, all through the turn: {worst} m off at worst"
    );
}

#[test]
fn shown_as_its_clips_alone_have_it_the_gun_stays_where_they_show_it_walking_and_turning() {
    let mut own = scene_with(GunKind::BoltActionRifle);
    steady(&mut own);
    settle(&mut own, 40);
    let (arms, _) = clip_of_hold(&mut own);
    let shown = gun_from_eye(&own).w_axis.truncate();

    // Every layer on, as a holder has them, but shown as the clips alone have it.
    let mut scene = scene_with(GunKind::BoltActionRifle);
    steady(&mut scene);
    let holder = scene.world.get_mut::<GunHolder>(scene.player).unwrap();
    (holder.sway, holder.bob, holder.breathing, holder.base_only) = (1.0, 1.0, 1.0, true);
    held_by(&mut scene, &arms);
    settle(&mut scene, 40);
    let rest = rest_of(&scene);
    let mut time = Time { delta: 1.0 / 60.0, ..Time::default() };
    let mut most: f32 = 0.0;
    for frame in 0..90 {
        scene.world.get_mut::<Transform>(scene.player).unwrap().position.z += 3.0 / 60.0;
        scene.world.get_mut::<crate::first_person::FirstPersonView>(scene.player).unwrap().yaw +=
            if frame < 45 { 3.0 } else { -3.0 };
        step(&mut scene, &rest, &mut time);
        most = most.max(gun_from_eye(&scene).w_axis.truncate().distance(shown));
    }
    assert!(most < 0.0005, "the gun where the clip shows it, all along: {most} m off at most");
}

/// The clip of `arms` remade as `seconds` long, its gun lifted `up(t)`
/// metres `t` seconds in.
fn lifted(arms: &Arc<ModelAnimation>, seconds: f32, up: impl Fn(f32) -> f32) -> Arc<ModelAnimation> {
    let carrier = arms.skeleton.bones.len() - 1;
    let still = arms.clips[0].frame(0).to_vec();
    let frames = (seconds * voxear_render::CLIP_RATE).round() as usize + 1;
    let poses = (0..frames)
        .flat_map(|frame| {
            let mut pose = still.clone();
            pose[carrier].translation.y += up(frame as f32 / voxear_render::CLIP_RATE);
            pose
        })
        .collect();
    let clip = Clip { name: "Lifted".into(), frames, poses };
    Arc::new(ModelAnimation { skeleton: arms.skeleton.clone(), skins: Vec::new(), clips: vec![clip] })
}

#[test]
fn a_clip_that_snaps_a_hand_back_at_its_very_end_is_found_letting_go_as_the_hand_sets_off() {
    let mut own = scene_with(GunKind::BoltActionRifle);
    steady(&mut own);
    settle(&mut own, 40);
    let (arms, _) = clip_of_hold(&mut own);
    // The gun jumps 10 cm against the hands in the last tenth of a second of one clip, and over the last 0.6 s of
    // the other: on the gun, the hands snap back in the first and come back slowly in the second.
    let jump = |over: f32| move |t: f32| ((t - (1.0 - over)) / over).clamp(0.0, 1.0) * 0.1;
    let mut clips = Clips::default();
    for (name, file) in [("Snaps", lifted(&arms, 1.0, jump(0.1))), ("Slow", lifted(&arms, 1.0, jump(0.6)))] {
        let source = ClipSource { animation: file, index: 0, settings: Default::default() };
        clips.put(&format!("{name}#clip:Lifted"), source);
    }
    own.world.insert_resource(clips);
    let place = super::handling::Place::default();
    let found = |name: &str| {
        let reference = format!("{name}#clip:Lifted");
        super::clip_hold::snaps_back_at(&own.world, &reference, ("", &place), Vec3::ZERO, 0.35).expect("read")
    };
    let snaps = found("Snaps").expect("a snap back");
    assert!((snaps - 0.9).abs() < 0.03, "found as the hands set off: {snaps} s");
    assert_eq!(found("Slow"), None, "a hand coming back slower than the Hand Return is left as the clip has it");
}

#[test]
fn setting_off_into_a_run_plays_its_start_clip_then_the_run_from_its_first_frame() {
    let mut own = scene_with(GunKind::BoltActionRifle);
    steady(&mut own);
    settle(&mut own, 40);
    let (arms, _) = clip_of_hold(&mut own);
    // The run holds the gun 10 cm up; its start lifts it 20 cm and brings it back down to there, over half a second.
    let start = |t: f32| if t < 0.25 { 0.8 * t } else { 0.2 - 0.4 * (t - 0.25) };
    let mut clips = Clips::default();
    let files = [("Idle", arms.clone()), ("Run", lifted(&arms, 1.0, |_| 0.1)), ("Start", lifted(&arms, 0.5, start))];
    for (name, file) in files {
        let source = ClipSource { animation: file, index: 0, settings: Default::default() };
        clips.put(&format!("{name}#clip:Lifted"), source);
    }
    let mut handling = GunHandling::default();
    handling.pack.idle = "Idle#clip:Lifted".into();
    handling.pack.run = "Run#clip:Lifted".into();
    handling.pack.run_start = "Start#clip:Lifted".into();
    (handling.pack.walk_speed, handling.pack.run_speed) = (0.5, 1.0);
    handling.pack.eyes = crate::first_person::FirstPersonView::default().eyes;

    let mut scene = scene_with(GunKind::BoltActionRifle);
    steady(&mut scene);
    scene.world.insert_resource(clips);
    put_handling(&mut scene.world, "Held", Arc::new(handling));
    scene.world.get_mut::<Gun>(scene.gun).unwrap().handling = "Held".into();
    settle(&mut scene, 40);
    let standing = gun_from_eye(&scene).w_axis.y;
    let rest = rest_of(&scene);
    let mut time = Time { delta: 1.0 / 60.0, ..Time::default() };
    let mut highest: f32 = 0.0;
    for _ in 0..80 {
        scene.world.get_mut::<Transform>(scene.player).unwrap().position.z += 3.0 / 60.0;
        step(&mut scene, &rest, &mut time);
        highest = highest.max(gun_from_eye(&scene).w_axis.y - standing);
    }
    assert!(highest > 0.17, "the start lifts the gun as high as it does: {highest} m");
    let running = gun_from_eye(&scene).w_axis.y - standing;
    assert!((running - 0.1).abs() < 0.01, "then the run holds it where the start left it: {running} m");
}

#[test]
fn the_camera_stays_put_on_the_body_while_clips_hold_the_gun_however_the_body_bobs() {
    let mut own = scene_with(GunKind::BoltActionRifle);
    steady(&mut own);
    settle(&mut own, 40);
    let (arms, _) = clip_of_hold(&mut own);
    let mut scene = scene_with(GunKind::BoltActionRifle);
    steady(&mut scene);
    held_by(&mut scene, &arms);
    settle(&mut scene, 40);
    let camera = |scene: &Scene| {
        let obj = voxear_scene::world_matrix(&scene.world, scene.player);
        obj.inverse().transform_point3(voxear_scene::world_matrix(&scene.world, scene.camera).w_axis.truncate())
    };
    let still = camera(&scene);
    // The body bobs four centimetres up and down and sways aside as a walk has it, a step every half second.
    let rest = rest_of(&scene);
    let mut time = Time { delta: 1.0 / 60.0, ..Time::default() };
    let mut most: f32 = 0.0;
    for frame in 0..60 {
        let phase = frame as f32 / 30.0 * std::f32::consts::TAU;
        let bobbed = Mat4::from_translation(Vec3::new(phase.sin() * 0.02, (phase * 2.0).sin() * 0.02, 0.0));
        let walking: Vec<Mat4> = rest.iter().map(|bone| bobbed * *bone).collect();
        step(&mut scene, &walking, &mut time);
        most = most.max(camera(&scene).distance(still));
    }
    assert!(most < 0.001, "the camera where it was on the body: {most} m off at most");

    // With the head hidden, the upper chest and collarbones round the low camera go too, but not the arms on them.
    scene.world.get_mut::<crate::first_person::FirstPersonView>(scene.player).unwrap().hide_head = true;
    step(&mut scene, &rest, &mut time);
    let animation = scene.world.get::<ModelRenderer>(scene.body).unwrap().animation.clone().unwrap();
    let rig = crate::animation::rig::Rig::find(&animation.skeleton).unwrap();
    let hidden = &scene.world.get::<voxear_render::HiddenBones>(scene.body).expect("bones hidden").bones;
    let clavicles = rig.arms.iter().filter_map(|arm| arm.clavicle);
    assert!(hidden[rig.head] && hidden[rig.chest()] && clavicles.into_iter().all(|bone| hidden[bone]));
    assert!(rig.arms.iter().all(|arm| !hidden[arm.upper] && !hidden[arm.hand]), "the arms stay");

    // The body's own idle turns its chest and drops a shoulder; while the clips hold the gun the upper body stands as
    // their stance has it all the same, so the arms hang from shoulders that stay put.
    let skeleton = &animation.skeleton;
    let (chest, clavicle) = (rig.chest(), rig.arms[0].clavicle.unwrap());
    let turns = |scene: &Scene| {
        let posed = &scene.world.get::<Pose>(scene.body).unwrap().bones;
        [chest, clavicle].map(|bone| posed[bone].to_scale_rotation_translation().1)
    };
    step(&mut scene, &rest, &mut time);
    let standing = turns(&scene);
    let mut swaying = skeleton.rest();
    swaying[chest].rotation = Quat::from_rotation_y(0.3) * Quat::from_rotation_x(0.1);
    swaying[clavicle].rotation = Quat::from_rotation_z(0.25);
    step(&mut scene, &skeleton.model_matrices(&swaying), &mut time);
    for (now, was) in turns(&scene).into_iter().zip(standing) {
        assert!(now.angle_between(was) < 0.01, "stands as before: {} degrees off", now.angle_between(was).to_degrees());
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
