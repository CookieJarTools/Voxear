//! Creates a project's startup Objs, with their meshes, lights and cameras.
//! A stand-in for loading scene assets, which arrive with Content (M2).

use anyhow::{Context, Result, bail};
use voxear_core::math::{EulerRot, Quat, Vec3};
use voxear_render::{Camera, Light, MaterialDesc, MeshRenderer, Primitive, Renderer};
use voxear_scene::{Name, Parent, Transform};

use crate::{
    engine::Engine,
    project::{LightSpec, ObjSpec, Project},
};

/// Spawns every Obj in the project. Without a renderer (headless runs),
/// meshes are skipped but behaviours, cameras and lights are still created.
pub fn spawn_objs(engine: &mut Engine, mut renderer: Option<&mut Renderer>, project: &Project) -> Result<()> {
    for spec in &project.file.objs {
        let entity = engine.spawn_obj(&spec.name, transform(spec), &spec.behaviours)?;
        if let Some(camera) = &spec.camera {
            engine.world.insert(entity, Camera { fov_y_degrees: camera.fov, exposure: camera.exposure, ..Camera::default() });
        }
        if let Some(light) = &spec.light {
            engine.world.insert(entity, light_component(light));
        }
        let (Some(mesh), Some(renderer)) = (&spec.mesh, renderer.as_deref_mut()) else { continue };
        attach_mesh(engine, renderer, project, spec, entity, mesh)
            .with_context(|| format!("loading the mesh for {}", spec.name))?;
    }
    Ok(())
}

fn transform(spec: &ObjSpec) -> Transform {
    let [x, y, z] = spec.rotation.map(f32::to_radians);
    // Unity applies Z, then X, then Y: R = Ry · Rx · Rz.
    let rotation = Quat::from_euler(EulerRot::YXZ, y, x, z);
    Transform::new(Vec3::from(spec.position), rotation, Vec3::from(spec.scale))
}

fn attach_mesh(
    engine: &mut Engine,
    renderer: &mut Renderer,
    project: &Project,
    spec: &ObjSpec,
    entity: voxear_ecs::Entity,
    mesh: &str,
) -> Result<()> {
    let primitive = match mesh {
        "cube" => Some(Primitive::Cube),
        "sphere" => Some(Primitive::Sphere),
        "plane" => Some(Primitive::Plane),
        _ => None,
    };
    if let Some(primitive) = primitive {
        let mesh = renderer.primitive(primitive)?;
        let material = renderer.add_material(&material(spec))?;
        engine.world.insert(entity, MeshRenderer { mesh, material });
        return Ok(());
    }

    let path = project.root.join(mesh);
    let is_gltf = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("gltf") || e.eq_ignore_ascii_case("glb"));
    if !is_gltf {
        bail!("`{mesh}` is not a primitive (cube, sphere, plane) or a .gltf/.glb file");
    }
    // Each part of the model becomes a child Obj, so behaviours on the root move it all.
    for node in renderer.import_gltf(&path)? {
        engine.world.spawn((
            Name(format!("{}/{}", spec.name, node.name)),
            Transform::new(node.position, node.rotation, node.scale),
            Parent(entity),
            MeshRenderer { mesh: node.mesh, material: node.material },
        ));
    }
    Ok(())
}

fn material(spec: &ObjSpec) -> MaterialDesc {
    let defaults = MaterialDesc::default();
    let m = &spec.material;
    let [r, g, b] = linear(m.color.unwrap_or([0.8; 3]));
    MaterialDesc {
        base_color: [r, g, b, 1.0],
        metallic: m.metallic.unwrap_or(defaults.metallic),
        roughness: m.roughness.unwrap_or(defaults.roughness),
        emissive: m.emissive.unwrap_or(defaults.emissive),
        ..defaults
    }
}

fn light_component(spec: &LightSpec) -> Light {
    match *spec {
        LightSpec::Directional { color, intensity } => Light::directional(Vec3::from(linear(color)), intensity),
        LightSpec::Point { color, intensity, range } => Light::point(Vec3::from(linear(color)), intensity, range),
        LightSpec::Spot { color, intensity, range, inner_angle, outer_angle } => {
            Light::spot(Vec3::from(linear(color)), intensity, range, inner_angle, outer_angle)
        }
    }
}

/// Colours in project files are sRGB, as picked in any colour picker, like
/// Unity's inspector. Shading happens in linear space.
fn linear(srgb: [f32; 3]) -> [f32; 3] {
    srgb.map(|c| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) })
}
