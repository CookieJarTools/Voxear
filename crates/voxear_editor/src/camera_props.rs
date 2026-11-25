//! A Camera's settings past its projection, field of view or size, near
//! plane and exposure, in the Inspector and for `field.set`: how its field of
//! view is measured, a physical camera's lens, how far it sees, where on the
//! screen it draws and in what order, what it draws into, its background,
//! its own finish, the camera it is drawn over and the Obj it shows its
//! picture on (see `voxear_render::camera`). Which layers it sees are rows of
//! their own (`Prop::CameraSees`), as a Portal's are.

use voxear_ecs::{Entity, World};
use voxear_engine::Game;
use voxear_render::{AntiAliasingMode, Background, Camera, CameraTarget, FovAxis, GateFit, PhysicalLens, Projection};

use crate::props::{Prop, Value};

/// A value of a Camera beyond its first rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CameraField {
    FovAxis,
    Physical,
    FocalLength,
    Sensor,
    GateFit,
    LensShift,
    Far,
    Viewport,
    Priority,
    Target,
    PictureSize,
    ShowsOn,
    Background,
    BackgroundColor,
    Post,
    AntiAliasing,
    Hdr,
    OcclusionCulling,
    Over,
    ClipPlane,
}

use CameraField as F;

/// The displays a camera can draw to.
const DISPLAYS: u8 = 8;
const AA: [Option<AntiAliasingMode>; 4] =
    [None, Some(AntiAliasingMode::Off), Some(AntiAliasingMode::Fxaa), Some(AntiAliasingMode::Taa)];

/// The rows after the field of view or size, near plane and exposure, as
/// the camera's settings make them matter.
pub fn props(world: &World, entity: Entity) -> Vec<Prop> {
    let Some(camera) = world.get::<Camera>(entity) else { return Vec::new() };
    let mut fields = Vec::new();
    if camera.projection == Projection::Perspective {
        fields.push(F::Physical);
        match camera.physical {
            Some(_) => fields.extend([F::FocalLength, F::Sensor, F::GateFit, F::LensShift]),
            None => fields.push(F::FovAxis),
        }
    }
    fields.extend([F::Far, F::Viewport, F::Priority, F::Target]);
    if let CameraTarget::Picture { .. } = camera.target {
        fields.extend([F::PictureSize, F::ShowsOn]);
    }
    fields.push(F::Background);
    if let Background::Color { .. } = camera.background {
        fields.push(F::BackgroundColor);
    }
    fields.extend([F::Post, F::AntiAliasing, F::Hdr, F::OcclusionCulling, F::Over, F::ClipPlane]);
    let mut props: Vec<Prop> = fields.into_iter().map(Prop::Camera).collect();
    // Which layers it sees, a row for each the project names, after its finish.
    let layers = world.resource::<voxear_scene::Layers>();
    let at = props.iter().position(|prop| *prop == Prop::Camera(F::Post)).unwrap_or(props.len());
    let seen: Vec<Prop> = (0..voxear_scene::LAYER_COUNT as u8)
        .filter_map(|layer| {
            let name = layers.map_or(if layer == 0 { "Default" } else { "" }, |layers| layers.name(layer));
            (!name.is_empty()).then(|| Prop::CameraSees(layer, name.to_string()))
        })
        .collect();
    props.splice(at..at, seen);
    props
}

pub fn label(field: CameraField) -> String {
    match field {
        F::FovAxis => "Field of view axis",
        F::Physical => "Physical camera",
        F::FocalLength => "Focal length",
        F::Sensor => "Sensor size",
        F::GateFit => "Gate fit",
        F::LensShift => "Lens shift",
        F::Far => "Far",
        F::Viewport => "Viewport",
        F::Priority => "Priority",
        F::Target => "Draws to",
        F::PictureSize => "Picture size",
        F::ShowsOn => "Shown on",
        F::Background => "Background",
        F::BackgroundColor => "Background colour",
        F::Post => "Post-processing",
        F::AntiAliasing => "Anti-aliasing",
        F::Hdr => "HDR",
        F::OcclusionCulling => "Occlusion culling",
        F::Over => "Drawn over",
        F::ClipPlane => "Clip plane",
    }
    .into()
}

pub fn tooltip(field: CameraField) -> &'static str {
    match field {
        F::FovAxis => "Which way the field of view is measured: up and down, or across, so it sees as wide on every screen.",
        F::Physical => "Sets the field of view as a real camera's lens and sensor make it.",
        F::FocalLength => "The lens's focal length, in millimetres: longer sees less. 50 is about what an eye sees.",
        F::Sensor => "The sensor's width and height, in millimetres. 36 by 24 is full frame.",
        F::GateFit => "How the sensor is fitted to a screen of another shape: by its height, its width, whole, or filling it.",
        F::LensShift => "Moves the lens off the middle, as a share of the picture across and up, keeping lines upright.",
        F::Far => "How far it sees, in metres. 0 sees as far as there is anything.",
        F::Viewport => "Where on the screen it draws: left, bottom, width and height, from 0 to 1. Two cameras side by side split the screen.",
        F::Priority => "Which camera draws first: the lowest first, the highest on top.",
        F::Target => "The display it draws to, or a picture of its own that a screen in the world shows.",
        F::PictureSize => "The picture's width and height, in pixels.",
        F::ShowsOn => "The Obj whose mesh shows the picture, glowing as a screen does.",
        F::Background => "What fills where it sees nothing: the sky, one colour, or what was drawn there before.",
        F::BackgroundColor => "The colour of its background.",
        F::Post => "Whether the level's post-processing finishes its picture. Off, it is shown as lit.",
        F::AntiAliasing => "Its own anti-aliasing, or the post-processing's.",
        F::Hdr => "Keeps light brighter than white, for bloom and the tonemap. Off, it is cut at white.",
        F::OcclusionCulling => "Leaves out what is hidden behind nearer things.",
        F::Over => "Another camera it is drawn over, after it and in its place: a first-person view's hands over the world.",
        F::ClipPlane => "A plane, its way out and its place, nearer than which nothing is drawn, as a mirror's camera needs. All 0 for none.",
    }
}

/// The value as a scene file and `field.set` name it.
pub fn key(field: CameraField) -> &'static str {
    match field {
        F::FovAxis => "fov_axis",
        F::Physical => "physical",
        F::FocalLength => "focal_length",
        F::Sensor => "sensor",
        F::GateFit => "gate_fit",
        F::LensShift => "shift",
        F::Far => "far",
        F::Viewport => "viewport",
        F::Priority => "priority",
        F::Target => "target",
        F::PictureSize => "picture_size",
        F::ShowsOn => "shown_on",
        F::Background => "background",
        F::BackgroundColor => "background_color",
        F::Post => "post",
        F::AntiAliasing => "anti_aliasing",
        F::Hdr => "hdr",
        F::OcclusionCulling => "occlusion_culling",
        F::Over => "camera_over",
        F::ClipPlane => "clip_plane",
    }
}

pub fn unit(field: CameraField) -> Option<&'static str> {
    match field {
        F::FocalLength | F::Sensor => Some("mm"),
        F::Far => Some("m"),
        F::PictureSize => Some("px"),
        _ => None,
    }
}

/// Whether the row picks an Obj from a menu.
pub fn picks_obj(field: CameraField) -> bool {
    matches!(field, F::ShowsOn | F::Over)
}

fn target_choices() -> Vec<String> {
    (1..=DISPLAYS).map(|display| format!("Display {display}")).chain(["Picture".to_string()]).collect()
}

fn name_of(world: &World, entity: Option<Entity>) -> String {
    entity.and_then(|obj| world.get::<voxear_scene::Name>(obj)).map_or_else(|| "None".into(), |name| name.0.clone())
}

/// The Obj whose mesh shows a camera's picture.
pub fn shown_on(world: &World, camera: Entity) -> Option<Entity> {
    let screens = world.query_ref::<(Entity, &voxear_render::CameraScreen)>();
    screens.into_iter().find(|(_, screen)| screen.camera == camera).map(|(entity, _)| entity)
}

pub fn get(game: &Game, entity: Entity, field: CameraField) -> Option<Value> {
    let world = &game.engine.world;
    let camera = world.get::<Camera>(entity)?;
    let lens = camera.physical.unwrap_or_default();
    Some(match field {
        F::FovAxis => Value::Choice(
            FovAxis::ALL.iter().position(|axis| *axis == camera.fov_axis).unwrap_or(0),
            FovAxis::ALL.iter().map(|axis| axis.name().to_string()).collect(),
        ),
        F::Physical => Value::Toggle(camera.physical.is_some()),
        F::FocalLength => Value::Number(lens.focal_length),
        F::Sensor => Value::Vector(lens.sensor.to_vec()),
        F::GateFit => Value::Choice(
            GateFit::ALL.iter().position(|fit| *fit == lens.gate_fit).unwrap_or(0),
            GateFit::ALL.iter().map(|fit| fit.name().to_string()).collect(),
        ),
        F::LensShift => Value::Vector(lens.shift.to_vec()),
        F::Far => Value::Number(camera.far.unwrap_or(0.0)),
        F::Viewport => Value::Vector(camera.viewport.to_vec()),
        F::Priority => Value::Whole(i64::from(camera.priority)),
        F::Target => Value::Choice(
            match camera.target {
                CameraTarget::Display { display } => usize::from(display.min(DISPLAYS - 1)),
                CameraTarget::Picture { .. } => usize::from(DISPLAYS),
            },
            target_choices(),
        ),
        F::PictureSize => match camera.target {
            CameraTarget::Picture { width, height } => Value::Vector(vec![width as f32, height as f32]),
            CameraTarget::Display { .. } => return None,
        },
        F::ShowsOn => Value::Obj(name_of(world, shown_on(world, entity))),
        F::Background => Value::Choice(
            camera.background.index(),
            ["Sky", "Colour", "Nothing"].map(str::to_string).to_vec(),
        ),
        F::BackgroundColor => match camera.background {
            Background::Color { color } => Value::Color(color),
            _ => return None,
        },
        F::Post => Value::Toggle(camera.finish.post),
        F::AntiAliasing => Value::Choice(
            AA.iter().position(|mode| *mode == camera.finish.anti_aliasing).unwrap_or(0),
            ["Post-processing's", "Off", "FXAA", "TAA"].map(str::to_string).to_vec(),
        ),
        F::Hdr => Value::Toggle(camera.finish.hdr),
        F::OcclusionCulling => Value::Toggle(camera.finish.occlusion_culling),
        F::Over => Value::Obj(name_of(world, world.get::<voxear_render::CameraOver>(entity).map(|over| over.0))),
        F::ClipPlane => Value::Vector(camera.clip_plane.unwrap_or([0.0; 4]).to_vec()),
    })
}

/// Sets a value; an Obj picked from a menu is set by `pick_obj`.
pub fn set(game: &mut Game, entity: Entity, field: CameraField, value: Value) -> bool {
    let world = &mut game.engine.world;
    let Some(camera) = world.get_mut::<Camera>(entity) else { return false };
    let before = *camera;
    let mut lens = camera.physical.unwrap_or_default();
    let numbers = |v: &[f32], n: usize| (v.len() >= n).then(|| v[..n].to_vec());
    match (field, value) {
        (F::FovAxis, Value::Choice(at, _)) => camera.fov_axis = FovAxis::ALL[at.min(1)],
        (F::Physical, Value::Toggle(on)) => {
            // Made physical, it keeps the field of view it had.
            let focal = PhysicalLens::focal_length_for(camera.fov_y_degrees.to_radians(), lens.sensor[1]);
            camera.physical = on.then_some(PhysicalLens { focal_length: focal, ..lens });
        }
        (F::FocalLength, Value::Number(n)) => {
            lens.focal_length = n.max(0.1);
            camera.physical = Some(lens);
        }
        (F::Sensor, Value::Vector(v)) => {
            let Some(v) = numbers(&v, 2) else { return false };
            lens.sensor = [v[0].max(0.1), v[1].max(0.1)];
            camera.physical = Some(lens);
        }
        (F::GateFit, Value::Choice(at, _)) => {
            lens.gate_fit = GateFit::ALL[at.min(GateFit::ALL.len() - 1)];
            camera.physical = Some(lens);
        }
        (F::LensShift, Value::Vector(v)) => {
            let Some(v) = numbers(&v, 2) else { return false };
            lens.shift = [v[0].clamp(-2.0, 2.0), v[1].clamp(-2.0, 2.0)];
            camera.physical = Some(lens);
        }
        (F::Far, Value::Number(n)) => camera.far = (n > 0.0).then_some(n.max(camera.near * 2.0)),
        (F::Viewport, Value::Vector(v)) => {
            let Some(v) = numbers(&v, 4) else { return false };
            camera.viewport = [v[0], v[1], v[2], v[3]].map(|share| share.clamp(0.0, 1.0));
        }
        (F::Priority, Value::Whole(n)) => camera.priority = n.clamp(-1000, 1000) as i32,
        (F::Priority, Value::Number(n)) => camera.priority = n.round().clamp(-1000.0, 1000.0) as i32,
        (F::Target, Value::Choice(at, _)) => {
            camera.target = match at >= usize::from(DISPLAYS) {
                true => match camera.target {
                    picture @ CameraTarget::Picture { .. } => picture,
                    CameraTarget::Display { .. } => CameraTarget::Picture { width: 512, height: 512 },
                },
                false => CameraTarget::Display { display: at as u8 },
            }
        }
        (F::PictureSize, Value::Vector(v)) => {
            let Some(v) = numbers(&v, 2) else { return false };
            let side = |n: f32| n.round().clamp(1.0, 8192.0) as u32;
            camera.target = CameraTarget::Picture { width: side(v[0]), height: side(v[1]) };
        }
        (F::Background, Value::Choice(at, _)) => {
            camera.background = match at {
                1 => Background::Color { color: [0.0; 3] },
                2 => Background::Nothing,
                _ => Background::Sky,
            }
        }
        (F::BackgroundColor, Value::Color(color)) => {
            camera.background = Background::Color { color: color.map(|c| c.clamp(0.0, 1.0)) }
        }
        (F::Post, Value::Toggle(on)) => camera.finish.post = on,
        (F::AntiAliasing, Value::Choice(at, _)) => camera.finish.anti_aliasing = AA[at.min(AA.len() - 1)],
        (F::Hdr, Value::Toggle(on)) => camera.finish.hdr = on,
        (F::OcclusionCulling, Value::Toggle(on)) => camera.finish.occlusion_culling = on,
        (F::ClipPlane, Value::Vector(v)) => {
            let Some(v) = numbers(&v, 4) else { return false };
            let plane = [v[0], v[1], v[2], v[3]];
            camera.clip_plane = (plane != [0.0; 4]).then_some(plane);
        }
        _ => return false,
    }
    *camera != before
}

/// Picks the Obj a camera is drawn over, or the Obj its picture shows on;
/// none takes it off.
pub fn pick_obj(world: &mut World, entity: Entity, field: CameraField, obj: Option<Entity>) -> bool {
    let obj = obj.filter(|&obj| obj != entity);
    match field {
        F::Over => match obj.filter(|obj| world.has::<Camera>(*obj)) {
            Some(under) => world.insert(entity, voxear_render::CameraOver(under)),
            None => world.remove::<voxear_render::CameraOver>(entity).is_some(),
        },
        F::ShowsOn => {
            let had = shown_on(world, entity);
            if had == obj {
                return false;
            }
            if let Some(had) = had {
                world.remove::<voxear_render::CameraScreen>(had);
            }
            if let Some(screen) = obj {
                world.insert(screen, voxear_render::CameraScreen { camera: entity, brightness: 1.0 });
            }
            true
        }
        _ => false,
    }
}

/// The Objs a row of `field` may pick: other cameras to be drawn over, or
/// Objs with a mesh to show the picture on.
pub fn obj_choices(world: &World, entity: Entity, field: CameraField) -> Vec<Entity> {
    let named = world.query_ref::<(Entity, &voxear_scene::Name)>().without::<voxear_scene::Destroying>();
    named
        .into_iter()
        .map(|(other, _)| other)
        .filter(|other| *other != entity && !world.has::<voxear_scene::EditorOnly>(*other))
        .filter(|other| match field {
            F::Over => world.has::<Camera>(*other),
            _ => world.has::<voxear_render::MeshRenderer>(*other),
        })
        .collect()
}

/// The Obj a row picks now.
pub fn picked(world: &World, entity: Entity, field: CameraField) -> Option<Entity> {
    match field {
        F::Over => world.get::<voxear_render::CameraOver>(entity).map(|over| over.0),
        F::ShowsOn => shown_on(world, entity),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(world: &World, entity: Entity) -> Vec<Prop> {
        props(world, entity)
    }

    #[test]
    fn a_cameras_rows_follow_its_settings() {
        let mut world = World::new();
        let eye = world.spawn((voxear_scene::Transform::IDENTITY, Camera::default()));
        let shown = rows(&world, eye);
        assert!(shown.contains(&Prop::Camera(F::FovAxis)) && !shown.contains(&Prop::Camera(F::FocalLength)));
        assert!(shown.contains(&Prop::CameraSees(0, "Default".into())), "a row for the default layer");
        assert!(!shown.contains(&Prop::Camera(F::PictureSize)) && !shown.contains(&Prop::Camera(F::BackgroundColor)));
        let camera = world.get_mut::<Camera>(eye).unwrap();
        camera.physical = Some(PhysicalLens::default());
        camera.target = CameraTarget::Picture { width: 64, height: 64 };
        camera.background = Background::Color { color: [1.0; 3] };
        let shown = rows(&world, eye);
        for field in [F::FocalLength, F::Sensor, F::GateFit, F::LensShift, F::PictureSize, F::ShowsOn, F::BackgroundColor] {
            assert!(shown.contains(&Prop::Camera(field)), "{field:?} shows");
        }
        assert!(!shown.contains(&Prop::Camera(F::FovAxis)), "a physical camera's lens makes its field of view");
    }

    #[test]
    fn a_camera_is_stacked_over_another_and_shows_its_picture_on_one_screen() {
        let mut world = World::new();
        let base = world.spawn((voxear_scene::Transform::IDENTITY, Camera::default()));
        let hands = world.spawn((voxear_scene::Transform::IDENTITY, Camera::default()));
        let screen = world.spawn_empty();
        let other = world.spawn_empty();
        assert!(pick_obj(&mut world, hands, F::Over, Some(base)));
        assert_eq!(picked(&world, hands, F::Over), Some(base));
        assert!(!pick_obj(&mut world, hands, F::Over, Some(screen)) || picked(&world, hands, F::Over).is_none(), "only a camera");
        assert!(pick_obj(&mut world, base, F::ShowsOn, Some(screen)));
        assert!(pick_obj(&mut world, base, F::ShowsOn, Some(other)));
        assert_eq!(shown_on(&world, base), Some(other));
        assert!(world.get::<voxear_render::CameraScreen>(screen).is_none(), "the screen it showed on before lets it go");
        assert_eq!([key(F::Over), key(F::Far), key(F::Viewport)], ["camera_over", "far", "viewport"]);
    }
}
