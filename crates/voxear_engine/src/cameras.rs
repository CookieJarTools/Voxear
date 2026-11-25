//! Cameras for scripts: a Camera's field of view, near plane, exposure,
//! projection and orthographic size, as `Camera` in Voxear.Core reads and
//! writes them.

use voxear_ecs::{Entity, World};
use voxear_render::{Camera, MaterialHandle};
use voxear_scripting::ComponentKind;

// What `values` reads or writes, as `Camera` in Voxear.Core numbers them.
const HAS: i32 = 0;
const ADD: i32 = 1;
const REMOVE: i32 = 2;
const FIELD_OF_VIEW: i32 = 3;
const NEAR: i32 = 4;
const EXPOSURE: i32 = 5;
const FOLLOWS: i32 = 6;
const PROJECTION: i32 = 7;
const SIZE: i32 = 8;
const FOV_AXIS: i32 = 9;
const PHYSICAL: i32 = 10;
const FOCAL_LENGTH: i32 = 11;
const SENSOR: i32 = 12;
const GATE_FIT: i32 = 13;
const LENS_SHIFT: i32 = 14;
const FAR: i32 = 15;
const VIEWPORT: i32 = 16;
const PRIORITY: i32 = 17;
const LAYERS: i32 = 18;
const TARGET: i32 = 19;
const BACKGROUND: i32 = 20;
const POST: i32 = 21;
const ANTI_ALIASING: i32 = 22;
const HDR: i32 = 23;
const OCCLUSION_CULLING: i32 = 24;
const OVER: i32 = 25;
const CLIP_PLANE: i32 = 26;
// The custom projection's four columns, and taking it off.
const PROJECTION_COLUMN: i32 = 27;
const NO_CUSTOM_PROJECTION: i32 = 31;
// On the Obj that shows a camera's picture: the camera, and how bright.
const SCREEN: i32 = 32;
const SCREEN_BRIGHTNESS: i32 = 33;

/// Looks through the level's cameras for the next frame, as
/// `Renderer::look_through_cameras` does, and puts each camera's picture on
/// the screens that show it (`CameraScreen`). The game window, `vox render`
/// and the editor's play call it before each frame.
pub fn look_through(world: &mut World, renderer: &mut voxear_render::Renderer, base: voxear_render::ViewOptions) -> anyhow::Result<()> {
    renderer.look_through_cameras(world, base)?;
    show_screens(world, renderer)
}

/// The material each screen shows its camera's picture with, made once for
/// each picture and brightness, and the material it had before.
#[derive(Default)]
struct ScreenMaterials(std::collections::HashMap<Entity, (voxear_render::TextureId, u32, MaterialHandle, MaterialHandle)>);

/// Gives each Obj with a Camera Screen and a Mesh Renderer a material that
/// shows its camera's picture, glowing; and the one it had back when the
/// Camera Screen goes or has no picture.
pub fn show_screens(world: &mut World, renderer: &mut voxear_render::Renderer) -> anyhow::Result<()> {
    let screens: Vec<(Entity, voxear_render::CameraScreen)> =
        world.query_ref::<(Entity, &voxear_render::CameraScreen)>().into_iter().map(|(e, s)| (e, *s)).collect();
    if screens.is_empty() && world.resource::<ScreenMaterials>().is_none() {
        return Ok(());
    }
    let mut kept = world.remove_resource::<ScreenMaterials>().unwrap_or_default();
    // Screens gone, or without a picture now, get their own material back.
    let showing: Vec<Entity> = kept.0.keys().copied().collect();
    for entity in showing {
        let picture = screens.iter().find(|(e, _)| *e == entity).and_then(|(_, s)| renderer.camera_picture(s.camera));
        if picture.is_none() {
            let (.., own) = kept.0.remove(&entity).expect("listed");
            if let Some(mesh) = world.get_mut::<voxear_render::MeshRenderer>(entity) {
                mesh.material = own;
            }
        }
    }
    for (entity, screen) in screens {
        let Some(picture) = renderer.camera_picture(screen.camera) else { continue };
        let Some(mesh) = world.get::<voxear_render::MeshRenderer>(entity).copied() else { continue };
        let brightness = screen.brightness.max(0.0);
        let current = kept.0.get(&entity).copied();
        if current.is_some_and(|(texture, bits, ..)| texture == picture && bits == brightness.to_bits()) {
            continue;
        }
        let desc = voxear_render::MaterialDesc {
            base_color: [0.0, 0.0, 0.0, 1.0],
            emissive: [brightness; 3],
            emissive_texture: Some(picture),
            ..voxear_render::MaterialDesc::default()
        };
        let made = renderer.add_material(&desc)?;
        let own = current.map_or(mesh.material, |(.., own)| own);
        kept.0.insert(entity, (picture, brightness.to_bits(), made, own));
        if let Some(mesh) = world.get_mut::<voxear_render::MeshRenderer>(entity) {
            mesh.material = made;
        }
    }
    world.insert_resource(kept);
    Ok(())
}

pub(crate) fn register() {
    voxear_scripting::set_kind_values(ComponentKind::Camera, values);
}

fn values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    let number = |n: f32| [n, 0.0, 0.0, 0.0];
    match (what, written) {
        (HAS, None) => return Some(number(f32::from(u8::from(world.has::<Camera>(entity))))),
        (ADD, Some(_)) => {
            if !world.has::<Camera>(entity) {
                world.insert(entity, Camera::default());
            }
            return Some([0.0; 4]);
        }
        (REMOVE, Some(_)) => return world.remove::<Camera>(entity).map(|_| [0.0; 4]),
        // The Obj it films, which its view is carried through Portals to go on seeing (see `crate::portals`).
        (OVER, None) if world.has::<Camera>(entity) => {
            return Some(obj_value(world.get::<voxear_render::CameraOver>(entity).map(|over| over.0)));
        }
        (OVER, Some(value)) if world.has::<Camera>(entity) => {
            match value_obj(value).filter(|over| *over != entity && world.has::<Camera>(*over)) {
                Some(over) => {
                    world.insert(entity, voxear_render::CameraOver(over));
                }
                None => {
                    world.remove::<voxear_render::CameraOver>(entity);
                }
            }
            return Some([0.0; 4]);
        }
        (SCREEN, None) => return Some(obj_value(world.get::<voxear_render::CameraScreen>(entity).map(|s| s.camera))),
        (SCREEN_BRIGHTNESS, None) => {
            return world.get::<voxear_render::CameraScreen>(entity).map(|screen| number(screen.brightness));
        }
        (SCREEN, Some(value)) => {
            let brightness = world.get::<voxear_render::CameraScreen>(entity).map_or(1.0, |screen| screen.brightness);
            match value_obj(value).filter(|camera| world.has::<Camera>(*camera)) {
                Some(camera) => {
                    world.insert(entity, voxear_render::CameraScreen { camera, brightness });
                }
                None => {
                    world.remove::<voxear_render::CameraScreen>(entity);
                }
            }
            return Some([0.0; 4]);
        }
        (SCREEN_BRIGHTNESS, Some([x, ..])) => {
            let screen = world.get_mut::<voxear_render::CameraScreen>(entity)?;
            screen.brightness = x.max(0.0);
            return Some([0.0; 4]);
        }
        (FOLLOWS, None) if world.has::<Camera>(entity) => {
            return Some(obj_value(world.get::<voxear_render::CameraFollows>(entity).map(|follows| follows.0)));
        }
        (FOLLOWS, Some(value)) if world.has::<Camera>(entity) => {
            match value_obj(value).filter(|follows| *follows != entity) {
                Some(follows) => {
                    world.insert(entity, voxear_render::CameraFollows(follows));
                }
                None => {
                    world.remove::<voxear_render::CameraFollows>(entity);
                }
            }
            return Some([0.0; 4]);
        }
        _ => {}
    }
    let camera = world.get_mut::<Camera>(entity)?;
    match (what, written) {
        (FIELD_OF_VIEW, None) => Some(number(camera.fov_y_degrees)),
        (NEAR, None) => Some(number(camera.near)),
        (EXPOSURE, None) => Some(number(camera.exposure)),
        (PROJECTION, None) => Some(number(f32::from(camera.projection == voxear_render::Projection::Orthographic))),
        (SIZE, None) => Some(number(camera.size)),
        (PROJECTION, Some([x, ..])) => {
            camera.projection = match x >= 0.5 {
                true => voxear_render::Projection::Orthographic,
                false => voxear_render::Projection::Perspective,
            };
            Some([0.0; 4])
        }
        (SIZE, Some([x, ..])) => {
            camera.size = x.max(1e-3);
            Some([0.0; 4])
        }
        (written, value) => lens_and_finish(camera, written, value),
    }
}

/// The rest of a Camera's values: its lens, where and what it draws, and its
/// own finish.
fn lens_and_finish(camera: &mut Camera, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    use voxear_render::{AntiAliasingMode, Background, CameraTarget, FovAxis, GateFit, PhysicalLens};
    let flag = |on: bool| [f32::from(u8::from(on)), 0.0, 0.0, 0.0];
    let done = Some([0.0; 4]);
    let physical = camera.physical.unwrap_or_default();
    let modes = [AntiAliasingMode::Off, AntiAliasingMode::Fxaa, AntiAliasingMode::Taa];
    match (what, written) {
        (FOV_AXIS, None) => Some(flag(camera.fov_axis == FovAxis::Horizontal)),
        (FOV_AXIS, Some([x, ..])) => {
            camera.fov_axis = if x >= 0.5 { FovAxis::Horizontal } else { FovAxis::Vertical };
            done
        }
        (PHYSICAL, None) => Some(flag(camera.physical.is_some())),
        (PHYSICAL, Some([x, ..])) => {
            camera.physical = (x >= 0.5).then_some(physical);
            done
        }
        (FOCAL_LENGTH, None) => Some([physical.focal_length, 0.0, 0.0, 0.0]),
        (SENSOR, None) => Some([physical.sensor[0], physical.sensor[1], 0.0, 0.0]),
        (GATE_FIT, None) => Some([GateFit::ALL.iter().position(|fit| *fit == physical.gate_fit).unwrap_or(0) as f32, 0.0, 0.0, 0.0]),
        (LENS_SHIFT, None) => Some([physical.shift[0], physical.shift[1], 0.0, 0.0]),
        // Setting any of a physical camera's values makes it one.
        (FOCAL_LENGTH | SENSOR | GATE_FIT | LENS_SHIFT, Some([x, y, ..])) => {
            let mut lens = physical;
            match what {
                FOCAL_LENGTH => lens.focal_length = x.max(0.1),
                SENSOR => lens.sensor = [x.max(0.1), y.max(0.1)],
                GATE_FIT => lens.gate_fit = GateFit::ALL[(x.round().max(0.0) as usize).min(GateFit::ALL.len() - 1)],
                _ => lens.shift = [x, y],
            }
            camera.physical = Some(PhysicalLens { ..lens });
            done
        }
        (FAR, None) => Some([camera.far.unwrap_or(0.0), 0.0, 0.0, 0.0]),
        (FAR, Some([x, ..])) => {
            camera.far = (x > 0.0).then_some(x.max(camera.near * 2.0));
            done
        }
        (VIEWPORT, None) => Some(camera.viewport),
        (VIEWPORT, Some(rect)) => {
            camera.viewport = rect.map(|share| share.clamp(0.0, 1.0));
            done
        }
        (PRIORITY, None) => Some([camera.priority as f32, 0.0, 0.0, 0.0]),
        (PRIORITY, Some([x, ..])) => {
            camera.priority = x.round() as i32;
            done
        }
        // A mask of 32 bits, bit for bit in a float.
        (LAYERS, None) => Some([f32::from_bits(camera.layers), 0.0, 0.0, 0.0]),
        (LAYERS, Some([x, ..])) => {
            camera.layers = x.to_bits();
            done
        }
        (TARGET, None) => Some(match camera.target {
            CameraTarget::Display { display } => [0.0, f32::from(display), 0.0, 0.0],
            CameraTarget::Picture { width, height } => [1.0, width as f32, height as f32, 0.0],
        }),
        (TARGET, Some([kind, a, b, _])) => {
            camera.target = match kind >= 0.5 {
                true => CameraTarget::Picture { width: a.clamp(1.0, 8192.0) as u32, height: b.clamp(1.0, 8192.0) as u32 },
                false => CameraTarget::Display { display: a.clamp(0.0, 7.0) as u8 },
            };
            done
        }
        (BACKGROUND, None) => Some(match camera.background {
            Background::Sky => [0.0; 4],
            Background::Color { color: [r, g, b] } => [1.0, r, g, b],
            Background::Nothing => [2.0, 0.0, 0.0, 0.0],
        }),
        (BACKGROUND, Some([kind, r, g, b])) => {
            camera.background = match kind.round() as i32 {
                1 => Background::Color { color: [r, g, b].map(|c| c.clamp(0.0, 1.0)) },
                2 => Background::Nothing,
                _ => Background::Sky,
            };
            done
        }
        (POST, None) => Some(flag(camera.finish.post)),
        (POST, Some([x, ..])) => {
            camera.finish.post = x >= 0.5;
            done
        }
        // -1 for the post-processing's own.
        (ANTI_ALIASING, None) => {
            let at = camera.finish.anti_aliasing.and_then(|mode| modes.iter().position(|m| *m == mode));
            Some([at.map_or(-1.0, |at| at as f32), 0.0, 0.0, 0.0])
        }
        (ANTI_ALIASING, Some([x, ..])) => {
            camera.finish.anti_aliasing = (x >= 0.0).then(|| modes[(x.round() as usize).min(modes.len() - 1)]);
            done
        }
        (HDR, None) => Some(flag(camera.finish.hdr)),
        (HDR, Some([x, ..])) => {
            camera.finish.hdr = x >= 0.5;
            done
        }
        (OCCLUSION_CULLING, None) => Some(flag(camera.finish.occlusion_culling)),
        (OCCLUSION_CULLING, Some([x, ..])) => {
            camera.finish.occlusion_culling = x >= 0.5;
            done
        }
        // All four 0 for none.
        (CLIP_PLANE, None) => Some(camera.clip_plane.unwrap_or([0.0; 4])),
        (CLIP_PLANE, Some(plane)) => {
            camera.clip_plane = (plane != [0.0; 4]).then_some(plane);
            done
        }
        (column @ PROJECTION_COLUMN..=30, None) => {
            let matrix = camera.custom_projection?;
            Some(matrix.col((column - PROJECTION_COLUMN) as usize).to_array())
        }
        // A column set starts from the projection its settings make, for a 16 by 9 picture.
        (column @ PROJECTION_COLUMN..=30, Some(values)) => {
            let mut matrix = camera.custom_projection.unwrap_or_else(|| voxear_render::Lens::of(camera, 16.0 / 9.0).projection());
            *matrix.col_mut((column - PROJECTION_COLUMN) as usize) = voxear_core::math::Vec4::from_array(values);
            camera.custom_projection = Some(matrix);
            done
        }
        (NO_CUSTOM_PROJECTION, Some(_)) => {
            camera.custom_projection = None;
            done
        }
        (NO_CUSTOM_PROJECTION, None) => Some(flag(camera.custom_projection.is_none())),
        (FIELD_OF_VIEW, Some([x, ..])) => {
            camera.fov_y_degrees = x.clamp(1.0, 170.0);
            Some([0.0; 4])
        }
        (NEAR, Some([x, ..])) => {
            camera.near = x.max(1e-4);
            Some([0.0; 4])
        }
        (EXPOSURE, Some([x, ..])) => {
            camera.exposure = x.max(0.0);
            Some([0.0; 4])
        }
        _ => None,
    }
}

/// An Obj as a script value: its id's two halves, bit for bit.
pub(crate) fn obj_value(entity: Option<Entity>) -> [f32; 4] {
    let bits = entity.map_or(0, Entity::to_bits);
    [f32::from_bits(bits as u32), f32::from_bits((bits >> 32) as u32), 0.0, 0.0]
}

/// The Obj a script value names, if any.
pub(crate) fn value_obj([low, high, ..]: [f32; 4]) -> Option<Entity> {
    let bits = u64::from(low.to_bits()) | (u64::from(high.to_bits()) << 32);
    (bits != 0).then(|| Entity::from_bits(bits))
}
