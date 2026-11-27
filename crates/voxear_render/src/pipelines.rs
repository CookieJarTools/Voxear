use std::{cell::RefCell, ffi::CStr};

use anyhow::{Context, Result, ensure};
use voxear_rhi::{Bindless, Blend, Gpu, GraphicsPipelineDesc, PipelineJob, ShaderModule, make_pipelines, vk};

use crate::{atmosphere::TABLE_FORMAT, hot_reload::ShaderSet, volumetric::VOLUME_FORMAT};

pub const HDR_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;
pub const DEPTH_FORMAT: vk::Format = vk::Format::D32_SFLOAT;
pub const PYRAMID_FORMAT: vk::Format = vk::Format::R32_SFLOAT;
/// Screen motion since last frame, in UV units.
pub const VELOCITY_FORMAT: vk::Format = vk::Format::R16G16_SFLOAT;
/// Bloom needs range, not precision or alpha.
pub const BLOOM_FORMAT: vk::Format = vk::Format::B10G11R11_UFLOAT_PACK32;
/// The tonemapped image when a finishing pass still follows. Kept at 16 bits
/// so the finish's dither can still break up banding.
pub const LDR_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;
/// FSR's exposure, a 1×1 picture whose red is what the scene is multiplied by.
pub const FSR_EXPOSURE_FORMAT: vk::Format = vk::Format::R32G32B32A32_SFLOAT;
/// Ambient occlusion within a frame: a visibility from 0 to 1.
pub const AO_FORMAT: vk::Format = vk::Format::R8_UNORM;
/// Each pixel's normal, folded flat (octahedral) in red and green, and its
/// roughness in blue, which the depth passes write beside the depth, so
/// reflections skip what is too rough to show them and trace from the
/// surface's own smooth normal. Ten bits each, as eight would facet a
/// mirror's reflection. Alpha is 0 where two LODs crossfade, so ambient
/// occlusion knows its depth is from either.
pub const SURFACE_FORMAT: vk::Format = vk::Format::A2B10G10R10_UNORM_PACK32;
/// What each pixel's surface is, beside it, for a shader pack's passes that
/// read it: its kind in the top bits and what of it below, the bits held as
/// a float's (`depthFragment` in `shaders/occlusion.slang`), 0 for the sky.
pub const SURFACE_ID_FORMAT: vk::Format = vk::Format::R32_SFLOAT;
/// Its history, which blends a tenth of each frame in, so eight bits would
/// stop short of the value it is heading for.
pub const AO_HISTORY_FORMAT: vk::Format = vk::Format::R16_SFLOAT;

/// Slope-scaled bias for shadow casters. The forward pass also offsets
/// lookups along the normal, so this only has to remove what that misses.
const SHADOW_BIAS: (f32, f32) = (0.0, 1.5);
/// Lifts a mesh's edges off its own faces, towards the camera (depth is
/// reversed), so they are not lost in them.
const EDGE_BIAS: (f32, f32) = (1.0, 1.0);

/// The scene's pipelines with task and mesh shaders in place of vertex
/// shaders, for GPUs that have them (see `shaders/meshlet_task.slang`).
#[derive(Clone, Copy)]
pub struct MeshPipelines {
    pub shadow: vk::Pipeline,
    pub sun_pages: vk::Pipeline,
    pub local_shadow: vk::Pipeline,
    pub shadow_two_sided: vk::Pipeline,
    pub local_shadow_two_sided: vk::Pipeline,
    pub forward: vk::Pipeline,
    pub depth: vk::Pipeline,
    pub forward_on_depth: vk::Pipeline,
    pub wireframe: vk::Pipeline,
    pub edges: vk::Pipeline,
}

#[derive(Clone, Copy)]
pub struct Pipelines {
    /// The depth passes write each pixel's surface id beside its normal
    /// (`SURFACE_ID_FORMAT`), as a worn shader pack that reads them wants.
    pub surface_ids: bool,
    pub clusters: vk::Pipeline,
    pub cull: vk::Pipeline,
    /// Turns each instance draw into draws of its meshlets that survive culling.
    pub meshlet_cull: vk::Pipeline,
    pub pyramid: vk::Pipeline,
    /// Builds a level of the nearest-depth pyramid that screen-space
    /// reflections step through (`nearestMain` in `shaders/pyramid.slang`).
    pub nearest_pyramid: vk::Pipeline,
    pub shadow: vk::Pipeline,
    /// As `shadow`, for local lights' views, without clamping depth: what
    /// lies nearer a light than its near plane, as the flame or bulb round
    /// it, is clipped away rather than flattened onto it, where it would
    /// shade everything.
    pub local_shadow: vk::Pipeline,
    /// As `shadow` and `local_shadow`, culling no backs, for two-sided
    /// casters' lists (`LIST_TWO_SIDED_SHADOW`).
    pub shadow_two_sided: vk::Pipeline,
    pub local_shadow_two_sided: vk::Pipeline,
    /// As `shadow`, into the sun's virtual shadow pages, whose projection
    /// keeps up the way up rather than turning it down the screen, which
    /// turns their triangles' winding round.
    pub sun_pages: vk::Pipeline,
    /// A view of the kept atlas of what stands still copied into the local
    /// shadow atlas, for what moves to be drawn over (see `shadow_cache`).
    pub kept_copy: vk::Pipeline,
    /// The pages of the sun's virtual shadow maps the view's depth asks for
    /// (see `vsm`).
    pub vsm_mark: vk::Pipeline,
    /// What the view asks of the lamps' virtual shadow pages (see `lamp_vsm`).
    pub lamp_mark: vk::Pipeline,
    pub forward: vk::Pipeline,
    /// The opaque lists as depth alone, drawn first when ambient occlusion
    /// is on, so it can work from the frame's whole depth.
    pub depth: vk::Pipeline,
    /// The forward pass over the depth already drawn: it neither writes
    /// depth nor shades anything that depth hides.
    pub forward_on_depth: vk::Pipeline,
    /// `forward`, `depth` and `forward_on_depth` culling no backs, for the
    /// two-sided lists, which are always drawn whole, without mesh shaders.
    pub forward_two_sided: vk::Pipeline,
    pub depth_two_sided: vk::Pipeline,
    pub forward_on_depth_two_sided: vk::Pipeline,
    /// The forward pass's meshes as their edges, for the wireframe view.
    pub wireframe: vk::Pipeline,
    /// The meshes' edges drawn faintly over the lit scene, for the lit
    /// wireframe view.
    pub edges: vk::Pipeline,
    pub sky: vk::Pipeline,
    /// The atmosphere's tables (see `atmosphere`).
    pub transmittance: vk::Pipeline,
    pub multi_scattering: vk::Pipeline,
    pub sky_view: vk::Pipeline,
    pub aerial_perspective: vk::Pipeline,
    /// The sky's clouds and their shadows (see `clouds`).
    pub clouds: vk::Pipeline,
    pub cloud_shadows: vk::Pipeline,
    pub cloud_panorama: vk::Pipeline,
    /// Volumetric fog's passes: the volume, its integration, and laying it
    /// over the scene.
    pub volume_scatter: vk::Pipeline,
    pub volume_integrate: vk::Pipeline,
    pub volumetric_fog: vk::Pipeline,
    /// Ambient occlusion's passes: GTAO, its denoise and its temporal blend.
    pub gtao: vk::Pipeline,
    pub ao_denoise: vk::Pipeline,
    pub ao_temporal: vk::Pipeline,
    /// Contact shadows' march towards the sun through the depth buffer.
    pub contact_shadows: vk::Pipeline,
    /// Screen-space reflections' march through the depth buffer.
    pub reflections: vk::Pipeline,
    pub fog: vk::Pipeline,
    pub taa: vk::Pipeline,
    /// TAA that builds the output from a scene drawn smaller.
    pub taa_upscale: vk::Pipeline,
    pub dof: vk::Pipeline,
    pub motion_blur: vk::Pipeline,
    pub bloom_down: vk::Pipeline,
    pub bloom_up: vk::Pipeline,
    pub exposure: vk::Pipeline,
    /// The sun's light and the sky's, once a frame under the atmosphere.
    pub sky_light: vk::Pipeline,
    /// Pushes through foliage: springing back, and this frame's stamped.
    pub foliage_push: vk::Pipeline,
    /// Presses this frame's tracks into the map round the camera (see `tracks`).
    pub tracks: vk::Pipeline,
    /// Steps the simulated particles (see `particle_sim`).
    pub particle_sim: vk::Pipeline,
    /// Erosion's passes, run when the editor erodes a terrain (see `erosion`).
    pub erosion_flux: vk::Pipeline,
    pub erosion_water: vk::Pipeline,
    pub erosion_carve: vk::Pipeline,
    pub erosion_carry: vk::Pipeline,
    pub erosion_slump: vk::Pipeline,
    pub erosion_settle: vk::Pipeline,
    /// Glass: the copy of the picture it is drawn over, and its panes.
    pub glass_copy: vk::Pipeline,
    pub glass: vk::Pipeline,
    /// The sea's waves: the FFT's rows and columns, their mips, the depth
    /// it is drawn over, and its surface (see `water`).
    pub ocean_rows: vk::Pipeline,
    pub ocean_columns: vk::Pipeline,
    pub ocean_mip: vk::Pipeline,
    pub ocean_foam: vk::Pipeline,
    pub ocean_mips: vk::Pipeline,
    /// Trails on water: what came into the square round the camera cleared
    /// and the rest carried along the water's flow, then a step of the
    /// waves (see `water_trails`).
    pub water_trails_carry: vk::Pipeline,
    pub water_trails: vk::Pipeline,
    /// The sunlight the waves focus and spread under them (see `water`).
    pub caustics: vk::Pipeline,
    pub floor_depth: vk::Pipeline,
    pub foam_pattern: vk::Pipeline,
    pub water_depth_copy: vk::Pipeline,
    pub water: vk::Pipeline,
    pub water_pool: vk::Pipeline,
    pub water_stream: vk::Pipeline,
    /// The blobs lava's bursting bubbles throw up, and the bubbles' domes.
    pub lava_spatter: vk::Pipeline,
    pub lava_domes: vk::Pipeline,
    pub sea_floor: vk::Pipeline,
    pub sea_floor_under: vk::Pipeline,
    pub sea_floor_walls: vk::Pipeline,
    pub particles: vk::Pipeline,
    /// The tails particles draw behind them, and the ribbon through a
    /// system's particles.
    pub particle_trails: vk::Pipeline,
    pub particle_ribbon: vk::Pipeline,
    /// Particles drawn as meshes, into the depth as well.
    pub particle_meshes: vk::Pipeline,
    /// Lays the particles drawn at half size over the scene.
    pub particle_composite: vk::Pipeline,
    pub sea_spray: vk::Pipeline,
    /// The spray over Streams' rapids.
    pub stream_spray: vk::Pipeline,
    pub smoke_march: vk::Pipeline,
    pub smoke_composite: vk::Pipeline,
    pub smoke_resolve: vk::Pipeline,
    /// Dicing displaced surfaces: splitting their triangles, sending out
    /// each piece in view, and drawing the pieces as depth, shaded, and
    /// shaded over the depth they drew (see `dice`).
    pub dice_split: vk::Pipeline,
    pub dice_patch: vk::Pipeline,
    pub depth_diced: vk::Pipeline,
    pub forward_diced: vk::Pipeline,
    pub forward_on_depth_diced: vk::Pipeline,
    /// Moves skinned parts' vertices by their bones (see `skinning`).
    pub skinning: vk::Pipeline,
    pub shadow_diced: vk::Pipeline,
    /// Terrain's patches (see `terrain`): shaded, as depth, shaded over the
    /// depth passes' depth, as edges, and into the sun's cascades.
    pub terrain_forward: vk::Pipeline,
    pub terrain_depth: vk::Pipeline,
    pub terrain_forward_on_depth: vk::Pipeline,
    pub terrain_wireframe: vk::Pipeline,
    pub terrain_shadow: vk::Pipeline,
    /// As `terrain_shadow`, into lamps' views, without clamping depth.
    pub terrain_local_shadow: vk::Pipeline,
    /// Worlds of blocks' quads (see `blocks`), as terrain's: shaded, as
    /// depth, shaded over the depth passes' depth, as edges, and into the
    /// sun's cascades.
    pub block_forward: vk::Pipeline,
    pub block_depth: vk::Pipeline,
    pub block_forward_on_depth: vk::Pipeline,
    /// Voxel Volumes (`shaders/voxels.slang`): the coarse rays, the depth
    /// pass and the opaque one, each a triangle over the screen writing its
    /// own depth.
    pub voxel_coarse: vk::Pipeline,
    /// What voxels a volume holds at given cells, for checking the GPU's
    /// against the CPU's.
    pub voxel_probe: vk::Pipeline,
    pub voxel_depth: vk::Pipeline,
    pub voxel_forward: vk::Pipeline,
    /// The voxels' shadows on whatever the depth passes drew, traced from each pixel.
    pub voxel_shadows: vk::Pipeline,
    /// Which lamps of each pixel's cluster the voxels stand in the way of.
    pub voxel_lamp_shadows: vk::Pipeline,
    /// The voxels' see-through voxels over everything opaque.
    pub voxel_glass: vk::Pipeline,
    /// The light kept round the camera for the voxels: what lets it through each cell, and a step of spreading it.
    pub voxel_light_cells: vk::Pipeline,
    /// The sun's shadows of the voxels round the camera, from above, for fog, particles and water.
    pub voxel_sun_map: vk::Pipeline,
    pub voxel_light_sky: vk::Pipeline,
    pub voxel_light_spread: vk::Pipeline,
    /// Endless Voxel Volumes' columns made on the GPU (`shaders/voxel_world.slang`).
    pub voxel_world_columns: vk::Pipeline,
    pub voxel_world_tops: vk::Pipeline,
    pub voxel_world_trees: vk::Pipeline,
    pub voxel_world_edits: vk::Pipeline,
    pub voxel_world_candidates: vk::Pipeline,
    pub voxel_world_arguments: vk::Pipeline,
    pub voxel_world_coarse: vk::Pipeline,
    pub voxel_world_brick_arguments: vk::Pipeline,
    pub voxel_world_bricks: vk::Pipeline,
    pub voxel_world_free: vk::Pipeline,
    pub voxel_world_pack: vk::Pipeline,
    pub voxel_world_sectors: vk::Pipeline,
    /// A world generator run at places alone, for checking it against the CPU's.
    pub voxel_world_places: vk::Pipeline,
    pub block_wireframe: vk::Pipeline,
    pub block_shadow: vk::Pipeline,
    /// As `block_shadow`, for faces that are never cut out: depth alone,
    /// with no fragment stage.
    pub block_shadow_solid: vk::Pipeline,
    /// The colour the sun's light takes through leaves, from its cascades,
    /// for a shader pack that lets it through them (`shader_packs::record_leaf_light`).
    pub block_leaf_light: vk::Pipeline,
    /// As `block_shadow`, into local lights' views, without clamping depth.
    pub block_local_shadow: vk::Pipeline,
    /// Grass (see `grass`): each draw's command made afresh, the blades
    /// scattered, the counts kept within their room; then drawn as depth,
    /// shaded writing depth, and shaded over the depth passes'.
    pub grass_reset: vk::Pipeline,
    pub grass_scatter: vk::Pipeline,
    pub ground_cover_scatter: vk::Pipeline,
    pub grass_clamp: vk::Pipeline,
    pub grass_depth: vk::Pipeline,
    pub grass_forward: vk::Pipeline,
    pub grass_forward_on_depth: vk::Pipeline,
    /// The grass passes' for ground cover, whose plants are closed cubes: their backs never show.
    pub cover_depth: vk::Pipeline,
    pub cover_forward: vk::Pipeline,
    pub cover_forward_on_depth: vk::Pipeline,
    pub dry_mesh: vk::Pipeline,
    pub water_wall: vk::Pipeline,
    pub dry_mesh_wall: vk::Pipeline,
    /// Worlds of blocks' water (see `blocks` and `water`): how far each
    /// pixel's view goes before it leaves the water, then its faces.
    pub block_water_exits: vk::Pipeline,
    pub block_water: vk::Pipeline,
    /// The water over endless Voxel Volumes' rivers and lakes.
    pub voxel_water: vk::Pipeline,
    /// Their waterfalls, as sheets and as voxels thrown over their lips.
    pub voxel_falls: vk::Pipeline,
    pub voxel_drops: vk::Pipeline,
    /// The sea floor's voxels from straight above, their tops and their
    /// undersides.
    pub sea_floor_voxels: vk::Pipeline,
    pub sea_floor_voxels_under: vk::Pipeline,
    pub block_sea_inside: vk::Pipeline,
    /// Under water: what the view shows dimmed, then the water's own light.
    pub underwater: vk::Pipeline,
    pub lens_wetness: vk::Pipeline,
    /// Panes into the glass shadow atlas (`glass::record_shadows`).
    pub glass_shadow: vk::Pipeline,
    pub glass_shadow_mip: vk::Pipeline,
    /// Writes the LDR intermediate, for a finishing pass to read.
    pub composite_ldr: vk::Pipeline,
    /// Writes the output directly, when nothing follows.
    pub composite: vk::Pipeline,
    pub finish: vk::Pipeline,
    /// A picture drawn at a few pixels to the screen's many, enlarged to the
    /// view nearest, each drawn pixel a block of screen pixels (see `pixels`).
    pub enlarge: vk::Pipeline,
    /// A camera's background of one colour, and its light cut at white
    /// without HDR (see `camera`).
    pub background: vk::Pipeline,
    pub clamp_light: vk::Pipeline,
    /// The frame debugger's view of an image.
    pub frame_debug: vk::Pipeline,
    /// A display image fitted to a window (see `display`), one for each
    /// format a window may be in: the first window's own, HDR10's two and
    /// scRGB's.
    pub encode: [(vk::Format, vk::Pipeline); 4],
    /// AMD FSR 3.1's passes, where the GPU can run them (see `fsr3`), and the
    /// pass that writes FSR the frame's exposure.
    pub fsr3: Option<crate::fsr3::Fsr3Pipelines>,
    pub fsr_exposure: vk::Pipeline,
    /// Copies the scene before what is drawn over it, for FSR's reactive mask.
    pub fsr_copy: vk::Pipeline,
    pub ui: vk::Pipeline,
    /// A UI panel's UI into its texture, the panels over each view's
    /// finished picture, and each panel's texture's mips (see `ui_panels`).
    pub ui_panel: vk::Pipeline,
    pub ui_panels: vk::Pipeline,
    pub ui_panel_text: vk::Pipeline,
    pub ui_panel_mips: vk::Pipeline,
    /// Debug draw's lines, and its solid shapes, over the finished frame.
    pub debug: vk::Pipeline,
    pub debug_solid: vk::Pipeline,
    /// An object on its own, for its preview, and its resolve into the
    /// preview atlas.
    pub preview: vk::Pipeline,
    pub preview_resolve: vk::Pipeline,
    /// A map of a world of blocks, drawn as a small model of it (see `map`).
    pub map: vk::Pipeline,
    /// A Portal's opening drawn into a view's depth, as any surface (see
    /// `portals` and `shaders/portal.slang`).
    pub portal_surface: vk::Pipeline,
    /// The pixels an opening shows in, opened in the depth of the view
    /// through it.
    pub portal_mask: vk::Pipeline,
    /// An opening's mist, over what shows in it.
    pub portal_mist: vk::Pipeline,
    /// A view's depth as the passes after its scene read it, and the depth
    /// of a view through an opening laid over it.
    pub portal_depth: vk::Pipeline,
    pub portal_depth_over: vk::Pipeline,
    /// Lays the water's and glass's depth, drawn after the far sides, over the depth the passes after read.
    pub portal_depth_nearer: vk::Pipeline,
    /// What a lit Portal's far side shows, read into its picture and its
    /// average (see `portal_light`).
    pub portal_light: vk::Pipeline,
    /// A plant drawn into each picture of its impostor's atlas: its colour
    /// and its normals (see `impostors`).
    pub impostor_bake: vk::Pipeline,
    /// Where the GPU has mesh shaders.
    pub mesh: Option<MeshPipelines>,
    /// Ray-traced GI's rays and update passes, where the GPU has ray tracing.
    pub ddgi: Option<[vk::Pipeline; 2]>,
    /// The same, tracing rays through hierarchies in a compute shader, for
    /// GPUs without ray tracing.
    pub ddgi_software: [vk::Pipeline; 2],
    /// Reflections by the GPU's rays, through the GI's scene, where the GPU
    /// has ray tracing (`rayReflectionsFragment` in `shaders/ddgi.slang`).
    pub ray_reflections: Option<vk::Pipeline>,
    /// A reflection probe's capture, by the GPU's rays where it has them,
    /// and in software (`probeCaptureMain` in `shaders/ddgi.slang`).
    pub probe_capture: Option<vk::Pipeline>,
    pub probe_capture_software: vk::Pipeline,
    /// Sound rays (see `sound_rays`), by ray queries where the GPU has them,
    /// and in software.
    pub sound_occlusion: Option<vk::Pipeline>,
    pub sound_reverb: Option<vk::Pipeline>,
    pub sound_occlusion_software: vk::Pipeline,
    pub sound_reverb_software: vk::Pipeline,
    /// A lighting bake's texels and probes, by the GPU's rays where it has
    /// them, and in software (`shaders/light_bake.slang`).
    pub light_bake: Option<[vk::Pipeline; 3]>,
    pub light_bake_software: [vk::Pipeline; 3],
    /// A probe's capture's mips, and their filtering for its own
    /// (`shaders/probe_filter.slang`).
    pub probe_downsample: vk::Pipeline,
    pub probe_filter: vk::Pipeline,
}

impl Pipelines {
    /// Pipelines that draw the picture into `output_format`, and fit a
    /// display image to windows in `window_format` or an HDR format.
    pub fn new(gpu: &Gpu, bindless: &Bindless, formats: (vk::Format, vk::Format), shaders: &ShaderSet) -> Result<Self> {
        Self::new_showing(gpu, bindless, formats, shaders, &mut |_, _| {})
    }

    /// As [`new`](Self::new), telling `progress` now and then how many of
    /// the pipelines are made, of how many, while they are made: compiling
    /// them all takes the driver a while the first time.
    pub fn new_showing(
        gpu: &Gpu,
        bindless: &Bindless,
        formats: (vk::Format, vk::Format),
        shaders: &ShaderSet,
        progress: &mut dyn FnMut(usize, usize),
    ) -> Result<Self> {
        let layout = bindless.pipeline_layout;
        let module = |name: &str| ShaderModule::new(gpu, &format!("{name}.slang"), shaders.get(name)?);
        let modules = [
            module("cull")?,
            module("pyramid")?,
            module("shadow")?,
            module("forward")?,
            module("sky")?,
            module("clusters")?,
            module("post")?,
            module("exposure")?,
            module("ui")?,
            module("preview")?,
            module("debug")?,
            module("ambient_occlusion")?,
            module("volumetric")?,
            module("ddgi_software")?,
            module("glass")?,
            module("foliage_push")?,
            module("water")?,
            module("particles")?,
            module("smoke")?,
            module("dice")?,
            module("skinning")?,
            module("terrain")?,
            module("grass")?,
            module("tracks")?,
            module("erosion")?,
            module("blocks")?,
            module("particle_sim")?,
            module("map")?,
            module("portal")?,
            module("probe_filter")?,
            module("voxels")?,
            module("voxel_world")?,
        ];
        // Only a GPU with mesh shaders can load their modules.
        let meshlets = match gpu.mesh_shader {
            Some(_) => Some([module("meshlet_task")?, module("meshlets")?, module("meshlet_shadow")?]),
            None => None,
        };
        // Only a GPU with ray tracing can load ray queries.
        let ddgi = match gpu.acceleration_structure {
            Some(_) => Some(module("ddgi")?),
            None => None,
        };
        let made = Self::make_all(gpu, layout, (&modules, &meshlets, &ddgi), (formats, shaders.surface_ids), progress);
        for module in modules.into_iter().chain(meshlets.into_iter().flatten()).chain(ddgi) {
            module.destroy(gpu);
        }
        made
    }

    /// Every pipeline noted down first, then all of them made at once on as
    /// many threads as there are, as each takes the driver a while to
    /// compile, and then handed back in the order they were asked for.
    fn make_all(
        gpu: &Gpu,
        layout: vk::PipelineLayout,
        modules: Modules,
        (formats, surface_ids): ((vk::Format, vk::Format), bool),
        progress: &mut dyn FnMut(usize, usize),
    ) -> Result<Self> {
        let noting = Maker::Noting(layout, RefCell::new(Vec::new()));
        Self::make(&noting, modules, formats, surface_ids)?;
        let Maker::Noting(_, jobs) = noting else { unreachable!("made noting") };
        let jobs = jobs.into_inner();
        let threads = pipeline_threads();
        let started = std::time::Instant::now();
        let (made, fsr3) = std::thread::scope(|scope| {
            let fsr3 = scope.spawn(|| match gpu.info.storage_without_format {
                true => crate::fsr3::Fsr3Pipelines::new(gpu).map(Some),
                false => Ok(None),
            });
            let made = make_pipelines(gpu, &jobs, threads, progress);
            (made, fsr3.join().unwrap_or_else(|_| Err(anyhow::anyhow!("making FSR's pipelines panicked"))))
        });
        let (made, fsr3) = match (made, fsr3) {
            (Ok(made), Ok(fsr3)) => (made, fsr3),
            (made, fsr3) => {
                made.iter().flatten().for_each(|&pipeline| unsafe { gpu.device.destroy_pipeline(pipeline, None) });
                fsr3.iter().flatten().for_each(|fsr3| fsr3.destroy(gpu));
                return Err(made.err().or(fsr3.err()).expect("one of them failed"));
            }
        };
        let took = started.elapsed().as_secs_f32();
        tracing::info!("made {} pipelines in {took:.1} s on {threads} threads", jobs.len());
        let names = jobs.iter().map(|job| job.name().to_string());
        let handing = Maker::Handing(RefCell::new(names.zip(made).collect::<Vec<_>>().into_iter()));
        let mut pipelines = Self::make(&handing, modules, formats, surface_ids)?;
        if let Maker::Handing(left) = &handing {
            ensure!(left.borrow().len() == 0, "fewer pipelines were asked for than were made");
        }
        pipelines.fsr3 = fsr3;
        Ok(pipelines)
    }

    /// The pipelines, each asked of `maker`.
    fn make(
        maker: &Maker,
        (modules, meshlets, ddgi): Modules,
        (output_format, window_format): (vk::Format, vk::Format),
        surface_ids: bool,
    ) -> Result<Self> {
        // The depth passes' targets: each pixel's normal and roughness, and its surface id where a pack reads them.
        let surface_formats: &[vk::Format] =
            if surface_ids { &[SURFACE_FORMAT, SURFACE_ID_FORMAT] } else { &[SURFACE_FORMAT] };
        let [
            cull,
            pyramid,
            shadow,
            forward,
            sky,
            clusters,
            post,
            exposure,
            ui,
            preview,
            debug,
            occlusion,
            volumetric,
            ddgi_software,
            glass,
            foliage_push,
            water,
            particles,
            smoke,
            dice,
            skinning,
            terrain,
            grass,
            tracks,
            erosion,
            blocks,
            particle_sim,
            map,
            portal,
            probe_filter,
            voxels,
            voxel_world,
        ] = modules;

        let graphics = |desc: &GraphicsPipelineDesc| maker.graphics(desc);
        let fullscreen = |name, entry, formats, blend| {
            graphics(&GraphicsPipelineDesc {
                color_formats: formats,
                cull: vk::CullModeFlags::NONE,
                blend,
                ..GraphicsPipelineDesc::new(name, post, c"postVertex", entry)
            })
        };
        let occlusion_pass = |name, entry, format: &'static [vk::Format]| {
            graphics(&GraphicsPipelineDesc {
                color_formats: format,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new(name, occlusion, c"aoVertex", entry)
            })
        };
        let table = |name, entry| {
            graphics(&GraphicsPipelineDesc {
                color_formats: &[TABLE_FORMAT],
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new(name, sky, c"lutVertex", entry)
            })
        };
        let volume = |name, entry| {
            graphics(&GraphicsPipelineDesc {
                color_formats: &[VOLUME_FORMAT],
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new(name, volumetric, c"volumeVertex", entry)
            })
        };
        let scene_formats = &[HDR_FORMAT, VELOCITY_FORMAT];
        let hdr = &[HDR_FORMAT];
        let bloom = &[BLOOM_FORMAT];
        let output = &[output_format];

        Ok(Self {
            surface_ids,
            clusters: maker.compute("Light clusters", clusters, c"clusterMain")?,
            cull: maker.compute("Cull", cull, c"cullMain")?,
            meshlet_cull: maker.compute("Meshlets", cull, c"meshletMain")?,
            pyramid: maker.compute("Depth pyramid", pyramid, c"reduceMain")?,
            nearest_pyramid: maker.compute("Nearest depth pyramid", pyramid, c"nearestMain")?,
            probe_downsample: maker.compute("Reflection probe mips", probe_filter, c"probeDownsampleMain")?,
            probe_filter: maker.compute("Reflection probe filter", probe_filter, c"probeFilterMain")?,
            probe_capture_software: maker.compute("Reflection probe capture", ddgi_software, c"probeCaptureMain")?,
            probe_capture: match &ddgi {
                Some(module) => Some(maker.compute("Reflection probe capture", module, c"probeCaptureMain")?),
                None => None,
            },
            sound_occlusion_software: maker.compute("Sound occlusion", ddgi_software, c"soundOcclusionMain")?,
            sound_reverb_software: maker.compute("Sound reverb", ddgi_software, c"soundReverbMain")?,
            sound_occlusion: match &ddgi {
                Some(module) => Some(maker.compute("Sound occlusion", module, c"soundOcclusionMain")?),
                None => None,
            },
            sound_reverb: match &ddgi {
                Some(module) => Some(maker.compute("Sound reverb", module, c"soundReverbMain")?),
                None => None,
            },
            light_bake_software: [
                maker.compute("Lightmap bake", ddgi_software, c"lightBakeTexelsMain")?,
                maker.compute("Light probe bake", ddgi_software, c"lightBakeProbesMain")?,
                maker.compute("Lightmap bake sky", ddgi_software, c"lightBakeSkyMain")?,
            ],
            light_bake: match &ddgi {
                Some(module) => Some([
                    maker.compute("Lightmap bake", module, c"lightBakeTexelsMain")?,
                    maker.compute("Light probe bake", module, c"lightBakeProbesMain")?,
                    maker.compute("Lightmap bake sky", module, c"lightBakeSkyMain")?,
                ]),
                None => None,
            },
            // Faces turned from the light cast nothing, as a one-sided card seen from behind lets the light through;
            // two-sided casters draw from lists of their own with the two-sided pipelines.
            shadow: graphics(&GraphicsPipelineDesc {
                depth_bias: Some(SHADOW_BIAS),
                depth_clamp: true,
                ..GraphicsPipelineDesc::depth_only("Shadows", shadow, c"shadowVertex", DEPTH_FORMAT)
            })?,
            local_shadow: graphics(&GraphicsPipelineDesc {
                depth_bias: Some(SHADOW_BIAS),
                ..GraphicsPipelineDesc::depth_only("Local shadows", shadow, c"shadowVertex", DEPTH_FORMAT)
            })?,
            sun_pages: graphics(&GraphicsPipelineDesc {
                front_face: vk::FrontFace::COUNTER_CLOCKWISE,
                depth_bias: Some(SHADOW_BIAS),
                depth_clamp: true,
                ..GraphicsPipelineDesc::depth_only("Sun shadow pages", shadow, c"shadowVertex", DEPTH_FORMAT)
            })?,
            shadow_two_sided: graphics(&GraphicsPipelineDesc {
                cull: vk::CullModeFlags::NONE,
                depth_bias: Some(SHADOW_BIAS),
                depth_clamp: true,
                ..GraphicsPipelineDesc::depth_only("Shadows (two-sided)", shadow, c"shadowVertex", DEPTH_FORMAT)
            })?,
            local_shadow_two_sided: graphics(&GraphicsPipelineDesc {
                cull: vk::CullModeFlags::NONE,
                depth_bias: Some(SHADOW_BIAS),
                ..GraphicsPipelineDesc::depth_only("Local shadows (two-sided)", shadow, c"shadowVertex", DEPTH_FORMAT)
            })?,
            vsm_mark: maker.compute("Virtual shadow requests", shadow, c"markMain")?,
            lamp_mark: maker.compute("Lamp shadow requests", shadow, c"markLampsMain")?,
            kept_copy: graphics(&GraphicsPipelineDesc {
                cull: vk::CullModeFlags::NONE,
                depth_compare: vk::CompareOp::ALWAYS,
                fragment_entry: Some(c"keptCopyFragment"),
                ..GraphicsPipelineDesc::depth_only("Kept local shadows", shadow, c"keptCopyVertex", DEPTH_FORMAT)
            })?,
            forward: graphics(&GraphicsPipelineDesc {
                color_formats: scene_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                ..GraphicsPipelineDesc::new("Opaque", forward, c"vertexMain", c"fragmentMain")
            })?,
            depth: graphics(&GraphicsPipelineDesc {
                color_formats: surface_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                ..GraphicsPipelineDesc::new("Depth", forward, c"vertexMain", c"depthFragment")
            })?,
            forward_on_depth: graphics(&GraphicsPipelineDesc {
                color_formats: scene_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                ..GraphicsPipelineDesc::new("Opaque", forward, c"vertexMain", c"fragmentMain")
            })?,
            forward_two_sided: graphics(&GraphicsPipelineDesc {
                color_formats: scene_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Opaque (two-sided)", forward, c"vertexMain", c"fragmentMain")
            })?,
            depth_two_sided: graphics(&GraphicsPipelineDesc {
                color_formats: surface_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Depth (two-sided)", forward, c"vertexMain", c"depthFragment")
            })?,
            forward_on_depth_two_sided: graphics(&GraphicsPipelineDesc {
                color_formats: scene_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Opaque (two-sided)", forward, c"vertexMain", c"fragmentMain")
            })?,
            wireframe: graphics(&GraphicsPipelineDesc {
                color_formats: scene_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                polygon_mode: vk::PolygonMode::LINE,
                ..GraphicsPipelineDesc::new("Wireframe", forward, c"vertexMain", c"fragmentMain")
            })?,
            edges: graphics(&GraphicsPipelineDesc {
                color_formats: hdr,
                depth_format: Some(DEPTH_FORMAT),
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                polygon_mode: vk::PolygonMode::LINE,
                blend: Blend::Alpha,
                depth_bias: Some(EDGE_BIAS),
                ..GraphicsPipelineDesc::new("Edges", forward, c"vertexMain", c"edgeFragment")
            })?,
            sky: graphics(&GraphicsPipelineDesc {
                color_formats: scene_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Sky", sky, c"skyVertex", c"skyFragment")
            })?,
            transmittance: table("Transmittance", c"transmittanceFragment")?,
            multi_scattering: table("Multiple scattering", c"multiScatteringFragment")?,
            sky_view: table("Sky view", c"skyViewFragment")?,
            aerial_perspective: table("Aerial perspective", c"aerialPerspectiveFragment")?,
            clouds: graphics(&GraphicsPipelineDesc {
                color_formats: &[TABLE_FORMAT, crate::clouds::DISTANCE_FORMAT],
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Clouds", sky, c"lutVertex", c"cloudsFragment")
            })?,
            cloud_shadows: table("Cloud shadows", c"cloudShadowsFragment")?,
            cloud_panorama: table("Cloud panorama", c"cloudPanoramaFragment")?,
            gtao: occlusion_pass("GTAO", c"gtaoFragment", &[AO_FORMAT])?,
            ao_denoise: occlusion_pass("AO denoise", c"aoDenoiseFragment", &[AO_FORMAT])?,
            ao_temporal: occlusion_pass("AO temporal", c"aoTemporalFragment", &[AO_HISTORY_FORMAT])?,
            contact_shadows: occlusion_pass("Contact shadows", c"contactShadowFragment", &[AO_FORMAT])?,
            reflections: occlusion_pass("Reflections", c"reflectionsFragment", &[HDR_FORMAT])?,
            fog: fullscreen("Fog", c"fogFragment", hdr, Blend::Alpha)?,
            volume_scatter: volume("Fog volume", c"volumeScatterFragment")?,
            volume_integrate: maker.compute("Fog integration", volumetric, c"volumeIntegrateMain")?,
            volumetric_fog: fullscreen("Volumetric fog", c"volumetricFogFragment", hdr, Blend::Premultiplied)?,
            taa: fullscreen("TAA", c"taaFragment", hdr, Blend::Opaque)?,
            taa_upscale: fullscreen("TAA upscale", c"taaUpscaleFragment", hdr, Blend::Opaque)?,
            dof: fullscreen("Depth of field", c"dofFragment", hdr, Blend::Opaque)?,
            motion_blur: fullscreen("Motion blur", c"motionBlurFragment", hdr, Blend::Opaque)?,
            bloom_down: fullscreen("Bloom down", c"bloomDownFragment", bloom, Blend::Opaque)?,
            bloom_up: fullscreen("Bloom up", c"bloomUpFragment", bloom, Blend::Additive)?,
            exposure: maker.compute("Exposure", exposure, c"exposureMain")?,
            glass_copy: maker.compute("Glass copy", glass, c"glassCopyMain")?,
            glass_shadow_mip: maker.compute("Glass shadow mips", glass, c"glassShadowMipMain")?,
            foliage_push: maker.compute("Foliage pushes", foliage_push, c"pushMain")?,
            tracks: maker.compute("Tracks", tracks, c"tracksMain")?,
            particle_sim: maker.compute("Particle simulation", particle_sim, c"simulateMain")?,
            erosion_flux: maker.compute("Erosion flow", erosion, c"erosionFlux")?,
            erosion_water: maker.compute("Erosion water", erosion, c"erosionWater")?,
            erosion_carve: maker.compute("Erosion carving", erosion, c"erosionCarve")?,
            erosion_carry: maker.compute("Erosion carrying", erosion, c"erosionCarry")?,
            erosion_slump: maker.compute("Erosion slumping", erosion, c"erosionSlump")?,
            erosion_settle: maker.compute("Erosion settling", erosion, c"erosionSettle")?,
            sky_light: maker.compute("Sky light", sky, c"skyLightMain")?,
            ocean_rows: maker.compute("Ocean rows", water, c"oceanRowsMain")?,
            ocean_columns: maker.compute("Ocean columns", water, c"oceanColumnsMain")?,
            ocean_mip: maker.compute("Ocean mips", water, c"oceanMipMain")?,
            ocean_foam: maker.compute("Ocean foam", water, c"oceanFoamMain")?,
            ocean_mips: maker.compute("Ocean mips, four at once", water, c"oceanMipsMain")?,
            water_trails_carry: maker.compute("Water trails carried", water, c"waterTrailsCarryMain")?,
            water_trails: maker.compute("Water trails", water, c"waterTrailsMain")?,
            caustics: maker.compute("Caustics", water, c"causticsMain")?,
            floor_depth: maker.compute("Sea floor depth", water, c"floorDepthMain")?,
            foam_pattern: maker.compute("Foam pattern", water, c"foamPatternMain")?,
            water_depth_copy: maker.compute("Water depth copy", water, c"waterDepthCopyMain")?,
            water: graphics(&GraphicsPipelineDesc {
                color_formats: &[HDR_FORMAT, VELOCITY_FORMAT],
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Water", water, c"waterVertex", c"waterFragment")
            })?,
            water_pool: graphics(&GraphicsPipelineDesc {
                color_formats: &[HDR_FORMAT, VELOCITY_FORMAT],
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Pools", water, c"poolVertex", c"waterFragment")
            })?,
            // Higher ground over lower: the depth is the height.
            sea_floor: graphics(&GraphicsPipelineDesc {
                cull: vk::CullModeFlags::NONE,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                ..GraphicsPipelineDesc::depth_only("Sea floor", water, c"floorVertex", DEPTH_FORMAT)
            })?,
            // Walls standing out of the sea, which straight above shows edge on: their triangles' edges, as lines,
            // their whole length kept, though their tops are over the floor's ceiling.
            sea_floor_walls: graphics(&GraphicsPipelineDesc {
                fragment_entry: Some(c"floorWallFragment"),
                cull: vk::CullModeFlags::NONE,
                polygon_mode: vk::PolygonMode::LINE,
                depth_clamp: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                ..GraphicsPipelineDesc::depth_only("Sea floor walls", water, c"floorVertex", DEPTH_FORMAT)
            })?,
            // Lower ground over higher: the undersides of what stands in the sea.
            sea_floor_under: graphics(&GraphicsPipelineDesc {
                cull: vk::CullModeFlags::NONE,
                depth_compare: vk::CompareOp::LESS_OR_EQUAL,
                ..GraphicsPipelineDesc::depth_only("Sea floor underside", water, c"floorVertex", DEPTH_FORMAT)
            })?,
            // Each pixel keeps the nearest front face and the farthest back face drawn.
            dry_mesh: graphics(&GraphicsPipelineDesc {
                color_formats: &[vk::Format::R32G32_SFLOAT],
                cull: vk::CullModeFlags::NONE,
                blend: Blend::Max,
                ..GraphicsPipelineDesc::new("Dry meshes", water, c"dryMeshVertex", c"dryMeshFragment")
            })?,
            water_stream: graphics(&GraphicsPipelineDesc {
                color_formats: &[HDR_FORMAT, VELOCITY_FORMAT],
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Streams", water, c"streamVertex", c"waterFragment")
            })?,
            lava_spatter: graphics(&GraphicsPipelineDesc {
                color_formats: &[HDR_FORMAT, VELOCITY_FORMAT],
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Lava spatter", water, c"lavaSpatterVertex", c"lavaSpatterFragment")
            })?,
            lava_domes: graphics(&GraphicsPipelineDesc {
                color_formats: &[HDR_FORMAT, VELOCITY_FORMAT],
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Lava bubbles", water, c"lavaDomeVertex", c"waterFragment")
            })?,
            water_wall: graphics(&GraphicsPipelineDesc {
                color_formats: &[HDR_FORMAT, VELOCITY_FORMAT],
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Walls of water", water, c"wallVertex", c"wallFragment")
            })?,
            // A mesh Dry Area's faces turned from the camera, told apart as the dry mask tells them.
            dry_mesh_wall: graphics(&GraphicsPipelineDesc {
                color_formats: &[HDR_FORMAT, VELOCITY_FORMAT],
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Mesh walls of water", water, c"dryMeshWallVertex", c"dryMeshWallFragment")
            })?,
            // The farthest face turned from the camera in front of what stands behind it.
            block_water_exits: graphics(&GraphicsPipelineDesc {
                color_formats: &[vk::Format::R32_SFLOAT],
                depth_format: Some(DEPTH_FORMAT),
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                blend: Blend::Max,
                ..GraphicsPipelineDesc::new(
                    "Water exits (blocks)",
                    water,
                    c"blockWaterVertex",
                    c"blockWaterExitFragment",
                )
            })?,
            // Seen from either side: its top's underside from in it.
            block_water: graphics(&GraphicsPipelineDesc {
                color_formats: &[HDR_FORMAT, VELOCITY_FORMAT],
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Water (blocks)", water, c"blockWaterVertex", c"blockWaterFragment")
            })?,
            // Seen from either side, as a world of blocks' water.
            voxel_water: graphics(&GraphicsPipelineDesc {
                color_formats: &[HDR_FORMAT, VELOCITY_FORMAT],
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Water (voxels)", water, c"voxelWaterVertex", c"voxelWaterFragment")
            })?,
            voxel_falls: graphics(&GraphicsPipelineDesc {
                color_formats: &[HDR_FORMAT, VELOCITY_FORMAT],
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Waterfalls (voxels)", water, c"voxelFallVertex", c"voxelFallFragment")
            })?,
            voxel_drops: graphics(&GraphicsPipelineDesc {
                color_formats: &[HDR_FORMAT, VELOCITY_FORMAT],
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Falling water (voxels)", water, c"voxelDropVertex", c"voxelDropFragment")
            })?,
            // Over the whole view, at the depth where each pixel's view meets the swell.
            block_sea_inside: graphics(&GraphicsPipelineDesc {
                color_formats: &[HDR_FORMAT, VELOCITY_FORMAT],
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new(
                    "Sea from in it (blocks)",
                    water,
                    c"underwaterVertex",
                    c"blockSeaInsideFragment",
                )
            })?,
            underwater: graphics(&GraphicsPipelineDesc {
                color_formats: hdr,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Under water", water, c"underwaterVertex", c"underwaterFragment")
            })?,
            lens_wetness: maker.compute("Lens wetness", water, c"lensWetnessMain")?,
            // Tested against the depth in its fragment shader, which reads it.
            glass: graphics(&GraphicsPipelineDesc {
                color_formats: hdr,
                ..GraphicsPipelineDesc::new("Glass", glass, c"glassVertex", c"glassFragment")
            })?,
            // Testing themselves against the depth they read, as glass does, and covering or adding to what is behind
            // as each system's alpha says: additive ones give no alpha.
            particles: graphics(&GraphicsPipelineDesc {
                color_formats: hdr,
                cull: vk::CullModeFlags::NONE,
                blend: Blend::Premultiplied,
                ..GraphicsPipelineDesc::new("Particles", particles, c"particleVertex", c"particleFragment")
            })?,
            particle_trails: graphics(&GraphicsPipelineDesc {
                color_formats: hdr,
                cull: vk::CullModeFlags::NONE,
                blend: Blend::Premultiplied,
                ..GraphicsPipelineDesc::new("Particle trails", particles, c"trailVertex", c"particleFragment")
            })?,
            particle_ribbon: graphics(&GraphicsPipelineDesc {
                color_formats: hdr,
                cull: vk::CullModeFlags::NONE,
                blend: Blend::Premultiplied,
                ..GraphicsPipelineDesc::new("Particle ribbons", particles, c"ribbonVertex", c"particleFragment")
            })?,
            // Meshes hide one another and what is behind them, so they test against the depth and draw into it; and
            // they say how far they moved, as the scene's surfaces do.
            particle_meshes: graphics(&GraphicsPipelineDesc {
                color_formats: scene_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                ..GraphicsPipelineDesc::new("Particle meshes", particles, c"particleMeshVertex", c"particleMeshFragment")
            })?,
            // Smoke marched at half size, its light and how much it lets through, then laid over the scene.
            smoke_march: graphics(&GraphicsPipelineDesc {
                color_formats: &[vk::Format::R16G16B16A16_SFLOAT, vk::Format::R32_SFLOAT],
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Smoke", smoke, c"smokeVertex", c"smokeMarchFragment")
            })?,
            smoke_resolve: graphics(&GraphicsPipelineDesc {
                color_formats: &[vk::Format::R16G16B16A16_SFLOAT],
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Smoke over frames", smoke, c"smokeVertex", c"smokeResolveFragment")
            })?,
            smoke_composite: graphics(&GraphicsPipelineDesc {
                color_formats: hdr,
                cull: vk::CullModeFlags::NONE,
                blend: Blend::Premultiplied,
                ..GraphicsPipelineDesc::new("Smoke over the scene", smoke, c"smokeVertex", c"smokeCompositeFragment")
            })?,
            dice_split: maker.compute("Dice (split)", dice, c"diceSplitMain")?,
            skinning: maker.compute("Skinning", skinning, c"skinMain")?,
            dice_patch: maker.compute("Dice (pieces)", dice, c"dicePatchMain")?,
            depth_diced: graphics(&GraphicsPipelineDesc {
                color_formats: surface_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                ..GraphicsPipelineDesc::new("Depth (diced)", dice, c"dicedVertex", c"depthFragment")
            })?,
            forward_diced: graphics(&GraphicsPipelineDesc {
                color_formats: scene_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                ..GraphicsPipelineDesc::new("Opaque (diced)", dice, c"dicedVertex", c"fragmentMain")
            })?,
            shadow_diced: graphics(&GraphicsPipelineDesc {
                cull: vk::CullModeFlags::NONE,
                depth_bias: Some(SHADOW_BIAS),
                depth_clamp: true,
                ..GraphicsPipelineDesc::depth_only("Shadows (diced)", dice, c"dicedShadowVertex", DEPTH_FORMAT)
            })?,
            forward_on_depth_diced: graphics(&GraphicsPipelineDesc {
                color_formats: scene_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                ..GraphicsPipelineDesc::new("Opaque (diced)", dice, c"dicedVertex", c"fragmentMain")
            })?,
            terrain_forward: graphics(&GraphicsPipelineDesc {
                color_formats: scene_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                ..GraphicsPipelineDesc::new("Opaque (terrain)", terrain, c"terrainVertex", c"fragmentMain")
            })?,
            terrain_depth: graphics(&GraphicsPipelineDesc {
                color_formats: surface_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                ..GraphicsPipelineDesc::new("Depth (terrain)", terrain, c"terrainVertex", c"depthFragment")
            })?,
            terrain_forward_on_depth: graphics(&GraphicsPipelineDesc {
                color_formats: scene_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                ..GraphicsPipelineDesc::new("Opaque (terrain)", terrain, c"terrainVertex", c"fragmentMain")
            })?,
            terrain_wireframe: graphics(&GraphicsPipelineDesc {
                color_formats: scene_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                polygon_mode: vk::PolygonMode::LINE,
                ..GraphicsPipelineDesc::new("Wireframe (terrain)", terrain, c"terrainVertex", c"fragmentMain")
            })?,
            terrain_shadow: graphics(&GraphicsPipelineDesc {
                // Ground seen edge on by the sun still casts.
                cull: vk::CullModeFlags::NONE,
                depth_bias: Some(SHADOW_BIAS),
                depth_clamp: true,
                // Its fragment shader cuts out the holes.
                fragment_entry: Some(c"terrainShadowFragment"),
                ..GraphicsPipelineDesc::depth_only("Shadows (terrain)", terrain, c"terrainShadowVertex", DEPTH_FORMAT)
            })?,
            terrain_local_shadow: graphics(&GraphicsPipelineDesc {
                cull: vk::CullModeFlags::NONE,
                depth_bias: Some(SHADOW_BIAS),
                fragment_entry: Some(c"terrainShadowFragment"),
                ..GraphicsPipelineDesc::depth_only(
                    "Local shadows (terrain)",
                    terrain,
                    c"terrainShadowVertex",
                    DEPTH_FORMAT,
                )
            })?,
            block_forward: graphics(&GraphicsPipelineDesc {
                color_formats: scene_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                ..GraphicsPipelineDesc::new("Opaque (blocks)", blocks, c"blockVertex", c"fragmentMain")
            })?,
            block_depth: graphics(&GraphicsPipelineDesc {
                color_formats: surface_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                ..GraphicsPipelineDesc::new("Depth (blocks)", blocks, c"blockVertex", c"depthFragment")
            })?,
            block_forward_on_depth: graphics(&GraphicsPipelineDesc {
                color_formats: scene_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                ..GraphicsPipelineDesc::new("Opaque (blocks)", blocks, c"blockVertex", c"fragmentMain")
            })?,
            voxel_coarse: maker.compute("Voxel coarse rays", voxels, c"voxelCoarse")?,
            voxel_light_cells: maker.compute("Voxel light cells", voxels, c"voxelLightCells")?,
            voxel_sun_map: maker.compute("Voxel sun map", voxels, c"voxelSunMap")?,
            voxel_light_sky: maker.compute("Voxel light sky", voxels, c"voxelLightSky")?,
            voxel_light_spread: maker.compute("Voxel light spread", voxels, c"voxelLightSpread")?,
            voxel_probe: maker.compute("Voxel probe", voxels, c"voxelProbe")?,
            voxel_world_columns: maker.compute("Voxel world columns", voxel_world, c"voxelWorldColumns")?,
            voxel_world_tops: maker.compute("Voxel world tops", voxel_world, c"voxelWorldTops")?,
            voxel_world_trees: maker.compute("Voxel world trees", voxel_world, c"voxelWorldTrees")?,
            voxel_world_edits: maker.compute("Voxel world edits", voxel_world, c"voxelWorldEdits")?,
            voxel_world_candidates: maker.compute("Voxel world candidates", voxel_world, c"voxelWorldCandidates")?,
            voxel_world_arguments: maker.compute("Voxel world arguments", voxel_world, c"voxelWorldArguments")?,
            voxel_world_coarse: maker.compute("Voxel world coarse", voxel_world, c"voxelWorldCoarse")?,
            voxel_world_brick_arguments: maker.compute(
                "Voxel world brick arguments",
                voxel_world,
                c"voxelWorldBrickArguments",
            )?,
            voxel_world_bricks: maker.compute("Voxel world bricks", voxel_world, c"voxelWorldBricks")?,
            voxel_world_free: maker.compute("Voxel world free", voxel_world, c"voxelWorldFree")?,
            voxel_world_pack: maker.compute("Voxel world pack", voxel_world, c"voxelWorldPack")?,
            voxel_world_sectors: maker.compute("Voxel world sectors", voxel_world, c"voxelWorldSectors")?,
            voxel_world_places: maker.compute("Voxel world places", voxel_world, c"voxelWorldPlaces")?,
            // The highest voxel over each texel, as the sea floor's meshes keep the highest; and the lowest beside it.
            sea_floor_voxels: graphics(&GraphicsPipelineDesc {
                cull: vk::CullModeFlags::NONE,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                depth_write: true,
                fragment_entry: Some(c"voxelFloorFragment"),
                ..GraphicsPipelineDesc::depth_only("Sea floor (voxels)", voxels, c"voxelVertex", DEPTH_FORMAT)
            })?,
            sea_floor_voxels_under: graphics(&GraphicsPipelineDesc {
                cull: vk::CullModeFlags::NONE,
                depth_compare: vk::CompareOp::LESS_OR_EQUAL,
                depth_write: true,
                fragment_entry: Some(c"voxelFloorFragment"),
                ..GraphicsPipelineDesc::depth_only("Sea floor underside (voxels)", voxels, c"voxelVertex", DEPTH_FORMAT)
            })?,
            voxel_depth: graphics(&GraphicsPipelineDesc {
                color_formats: surface_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Depth (voxels)", voxels, c"voxelVertex", c"voxelDepth")
            })?,
            voxel_forward: graphics(&GraphicsPipelineDesc {
                color_formats: scene_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Opaque (voxels)", voxels, c"voxelVertex", c"voxelOpaque")
            })?,
            voxel_glass: graphics(&GraphicsPipelineDesc {
                color_formats: hdr,
                depth_format: Some(DEPTH_FORMAT),
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                blend: Blend::Premultiplied,
                ..GraphicsPipelineDesc::new("Glass (voxels)", voxels, c"voxelVertex", c"voxelGlass")
            })?,
            voxel_shadows: graphics(&GraphicsPipelineDesc {
                color_formats: &[crate::voxel_volume::SUN_SHADOW_FORMAT],
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Voxel shadows", voxels, c"voxelVertex", c"voxelShadowFragment")
            })?,
            voxel_lamp_shadows: graphics(&GraphicsPipelineDesc {
                color_formats: &[crate::voxel_volume::LAMP_SHADOW_FORMAT],
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Voxel lamp shadows", voxels, c"voxelVertex", c"voxelLampShadowFragment")
            })?,
            block_wireframe: graphics(&GraphicsPipelineDesc {
                color_formats: scene_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                polygon_mode: vk::PolygonMode::LINE,
                ..GraphicsPipelineDesc::new("Wireframe (blocks)", blocks, c"blockVertex", c"fragmentMain")
            })?,
            block_shadow: graphics(&GraphicsPipelineDesc {
                // Faces seen edge on by the sun still cast, and those turned from it.
                cull: vk::CullModeFlags::NONE,
                depth_bias: Some(SHADOW_BIAS),
                depth_clamp: true,
                // Its fragment shader cuts leaves and other cut-out faces from their pictures.
                fragment_entry: Some(c"blockShadowFragment"),
                ..GraphicsPipelineDesc::depth_only("Shadows (blocks)", blocks, c"blockShadowVertex", DEPTH_FORMAT)
            })?,
            block_shadow_solid: graphics(&GraphicsPipelineDesc {
                cull: vk::CullModeFlags::NONE,
                depth_bias: Some(SHADOW_BIAS),
                depth_clamp: true,
                ..GraphicsPipelineDesc::depth_only("Shadows (solid blocks)", blocks, c"blockShadowVertex", DEPTH_FORMAT)
            })?,
            block_leaf_light: graphics(&GraphicsPipelineDesc {
                color_formats: &[crate::shader_packs::LEAF_LIGHT_FORMAT],
                cull: vk::CullModeFlags::NONE,
                blend: Blend::Filter,
                depth_clamp: true,
                ..GraphicsPipelineDesc::new("Leaf light", blocks, c"blockLeafLightVertex", c"blockLeafLightFragment")
            })?,
            block_local_shadow: graphics(&GraphicsPipelineDesc {
                cull: vk::CullModeFlags::NONE,
                depth_bias: Some(SHADOW_BIAS),
                fragment_entry: Some(c"blockShadowFragment"),
                ..GraphicsPipelineDesc::depth_only("Local shadows (blocks)", blocks, c"blockShadowVertex", DEPTH_FORMAT)
            })?,
            grass_reset: maker.compute("Grass reset", grass, c"grassReset")?,
            grass_scatter: maker.compute("Grass scatter", grass, c"grassScatter")?,
            ground_cover_scatter: maker.compute("Ground cover scatter", grass, c"groundCoverScatter")?,
            grass_clamp: maker.compute("Grass clamp", grass, c"grassClamp")?,
            // Blades are thin cards and tufts, seen from both sides.
            grass_depth: graphics(&GraphicsPipelineDesc {
                color_formats: surface_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Depth (grass)", grass, c"grassVertex", c"depthFragment")
            })?,
            grass_forward: graphics(&GraphicsPipelineDesc {
                color_formats: scene_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Opaque (grass)", grass, c"grassVertex", c"fragmentMain")
            })?,
            grass_forward_on_depth: graphics(&GraphicsPipelineDesc {
                color_formats: scene_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Opaque (grass)", grass, c"grassVertex", c"fragmentMain")
            })?,
            cover_depth: graphics(&GraphicsPipelineDesc {
                color_formats: surface_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                ..GraphicsPipelineDesc::new("Depth (grass)", grass, c"grassVertex", c"depthFragment")
            })?,
            cover_forward: graphics(&GraphicsPipelineDesc {
                color_formats: scene_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                ..GraphicsPipelineDesc::new("Opaque (grass)", grass, c"grassVertex", c"fragmentMain")
            })?,
            cover_forward_on_depth: graphics(&GraphicsPipelineDesc {
                color_formats: scene_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                ..GraphicsPipelineDesc::new("Opaque (grass)", grass, c"grassVertex", c"fragmentMain")
            })?,
            // The sea's spray: particles that read the waves and the foam, so with the sea's shaders.
            particle_composite: graphics(&GraphicsPipelineDesc {
                color_formats: hdr,
                cull: vk::CullModeFlags::NONE,
                blend: Blend::Premultiplied,
                ..GraphicsPipelineDesc::new("Particles over the scene", particles, c"particleScreenVertex", c"particleCompositeFragment")
            })?,
            sea_spray: graphics(&GraphicsPipelineDesc {
                color_formats: hdr,
                cull: vk::CullModeFlags::NONE,
                blend: Blend::Premultiplied,
                ..GraphicsPipelineDesc::new("Sea spray", water, c"sprayVertex", c"particleFragment")
            })?,
            stream_spray: graphics(&GraphicsPipelineDesc {
                color_formats: hdr,
                cull: vk::CullModeFlags::NONE,
                blend: Blend::Premultiplied,
                ..GraphicsPipelineDesc::new("Stream spray", water, c"streamSprayVertex", c"particleFragment")
            })?,
            glass_shadow: graphics(&GraphicsPipelineDesc {
                color_formats: &crate::glass::SHADOW_FORMATS,
                cull: vk::CullModeFlags::NONE,
                blend: Blend::Filter,
                // A pane nearer the sun than a cascade's near plane still filters its light.
                depth_clamp: true,
                ..GraphicsPipelineDesc::new("Glass shadows", glass, c"glassShadowVertex", c"glassShadowFragment")
            })?,
            composite_ldr: fullscreen("Composite", c"compositeFragment", &[LDR_FORMAT], Blend::Opaque)?,
            composite: fullscreen("Composite", c"compositeFragment", output, Blend::Opaque)?,
            finish: fullscreen("Finish", c"finishFragment", output, Blend::Opaque)?,
            enlarge: fullscreen("Enlarge", c"enlargeFragment", output, Blend::Opaque)?,
            background: fullscreen("Background", c"backgroundFragment", hdr, Blend::Opaque)?,
            clamp_light: fullscreen("Cut at white", c"clampFragment", hdr, Blend::Min)?,
            frame_debug: fullscreen("Frame debugger view", c"frameDebugFragment", hdr, Blend::Opaque)?,
            // Made on a thread of their own while the rest are made (see `new_showing`).
            fsr3: None,
            fsr_exposure: fullscreen("FSR exposure", c"fsrExposureFragment", &[FSR_EXPOSURE_FORMAT], Blend::Opaque)?,
            fsr_copy: fullscreen("FSR opaque copy", c"fsrCopyFragment", hdr, Blend::Opaque)?,
            encode: {
                let formats = [
                    window_format,
                    vk::Format::A2B10G10R10_UNORM_PACK32,
                    vk::Format::A2R10G10B10_UNORM_PACK32,
                    vk::Format::R16G16B16A16_SFLOAT,
                ];
                let mut encode = [(vk::Format::UNDEFINED, vk::Pipeline::null()); 4];
                for (made, format) in encode.iter_mut().zip(formats) {
                    let pipeline = graphics(&GraphicsPipelineDesc {
                        color_formats: std::slice::from_ref(&format),
                        cull: vk::CullModeFlags::NONE,
                        ..GraphicsPipelineDesc::new("Encode", post, c"postVertex", c"encodeFragment")
                    })?;
                    *made = (format, pipeline);
                }
                encode
            },
            ui: graphics(&GraphicsPipelineDesc {
                color_formats: output,
                cull: vk::CullModeFlags::NONE,
                blend: Blend::Premultiplied,
                ..GraphicsPipelineDesc::new("UI", ui, c"uiVertex", c"uiFragment")
            })?,
            ui_panel: graphics(&GraphicsPipelineDesc {
                color_formats: &[crate::ui_panels::PANEL_FORMAT],
                cull: vk::CullModeFlags::NONE,
                blend: Blend::Premultiplied,
                ..GraphicsPipelineDesc::new("UI panel", ui, c"uiVertex", c"uiFragment")
            })?,
            ui_panels: graphics(&GraphicsPipelineDesc {
                color_formats: output,
                cull: vk::CullModeFlags::NONE,
                blend: Blend::Premultiplied,
                ..GraphicsPipelineDesc::new("UI panels", ui, c"uiPanelVertex", c"uiPanelFragment")
            })?,
            ui_panel_text: graphics(&GraphicsPipelineDesc {
                color_formats: output,
                cull: vk::CullModeFlags::NONE,
                blend: Blend::Premultiplied,
                ..GraphicsPipelineDesc::new("UI panel text", ui, c"uiPanelTextVertex", c"uiPanelTextFragment")
            })?,
            ui_panel_mips: maker.compute("UI panel mips", ui, c"uiPanelMipMain")?,
            debug: graphics(&GraphicsPipelineDesc {
                color_formats: output,
                cull: vk::CullModeFlags::NONE,
                blend: Blend::Premultiplied,
                ..GraphicsPipelineDesc::new("Debug draw", debug, c"debugVertex", c"debugFragment")
            })?,
            debug_solid: graphics(&GraphicsPipelineDesc {
                color_formats: output,
                cull: vk::CullModeFlags::NONE,
                blend: Blend::Premultiplied,
                ..GraphicsPipelineDesc::new("Debug draw solids", debug, c"debugSolidVertex", c"debugSolidFragment")
            })?,
            preview: graphics(&GraphicsPipelineDesc {
                color_formats: hdr,
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                // Planes and open meshes show from either side.
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Preview", preview, c"previewVertex", c"previewFragment")
            })?,
            impostor_bake: graphics(&GraphicsPipelineDesc {
                color_formats: &[crate::impostors::IMPOSTOR_COLOR_FORMAT, crate::impostors::IMPOSTOR_NORMAL_FORMAT],
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                // Leaves show from either side.
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Impostor bake", preview, c"previewVertex", c"impostorBakeFragment")
            })?,
            preview_resolve: graphics(&GraphicsPipelineDesc {
                color_formats: &[crate::previews::PREVIEW_FORMAT],
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Preview resolve", preview, c"resolveVertex", c"resolveFragment")
            })?,
            map: graphics(&GraphicsPipelineDesc {
                color_formats: hdr,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Map", map, c"mapVertex", c"mapFragment")
            })?,
            // Depth is clamped for each, so an opening the camera stands in is not cut by the near plane.
            portal_surface: graphics(&GraphicsPipelineDesc {
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                depth_clamp: true,
                ..GraphicsPipelineDesc::depth_only("Portal surface", portal, c"portalVertex", DEPTH_FORMAT)
            })?,
            portal_mask: graphics(&GraphicsPipelineDesc {
                color_formats: &[],
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::ALWAYS,
                cull: vk::CullModeFlags::NONE,
                depth_clamp: true,
                ..GraphicsPipelineDesc::new("Portal mask", portal, c"portalVertex", c"portalMaskFragment")
            })?,
            portal_mist: graphics(&GraphicsPipelineDesc {
                color_formats: hdr,
                cull: vk::CullModeFlags::NONE,
                blend: Blend::Alpha,
                depth_clamp: true,
                ..GraphicsPipelineDesc::new("Portal mist", portal, c"portalVertex", c"portalMistFragment")
            })?,
            portal_depth: graphics(&GraphicsPipelineDesc {
                color_formats: &[PYRAMID_FORMAT],
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Portal depth", portal, c"portalDepthVertex", c"portalDepthFragment")
            })?,
            portal_depth_over: graphics(&GraphicsPipelineDesc {
                color_formats: &[PYRAMID_FORMAT],
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Portal depth over", portal, c"portalDepthVertex", c"portalDepthOverFragment")
            })?,
            portal_depth_nearer: graphics(&GraphicsPipelineDesc {
                color_formats: &[PYRAMID_FORMAT],
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new(
                    "Portal depth nearer",
                    portal,
                    c"portalDepthVertex",
                    c"portalDepthNearerFragment",
                )
            })?,
            portal_light: maker.compute("Portal light", portal, c"portalLightMain")?,
            mesh: meshlets
                .as_ref()
                .map(|modules| mesh_pipelines(maker, modules, (scene_formats, surface_formats)))
                .transpose()?,
            ddgi_software: [
                maker.compute("DDGI rays", ddgi_software, c"ddgiRaysMain")?,
                maker.compute("DDGI update", ddgi_software, c"ddgiUpdateMain")?,
            ],
            ray_reflections: match &ddgi {
                Some(module) => Some(graphics(&GraphicsPipelineDesc {
                    color_formats: &[HDR_FORMAT],
                    cull: vk::CullModeFlags::NONE,
                    ..GraphicsPipelineDesc::new("Reflections", module, c"aoVertex", c"rayReflectionsFragment")
                })?),
                None => None,
            },
            ddgi: match &ddgi {
                Some(module) => Some([
                    maker.compute("DDGI rays", module, c"ddgiRaysMain")?,
                    maker.compute("DDGI update", module, c"ddgiUpdateMain")?,
                ]),
                None => None,
            },
        })
    }

    pub fn destroy(&self, gpu: &Gpu) {
        let all = [
            self.clusters,
            self.cull,
            self.meshlet_cull,
            self.pyramid,
            self.shadow,
            self.local_shadow,
            self.shadow_two_sided,
            self.local_shadow_two_sided,
            self.sun_pages,
            self.kept_copy,
            self.vsm_mark,
            self.lamp_mark,
            self.forward,
            self.depth,
            self.forward_on_depth,
            self.forward_two_sided,
            self.depth_two_sided,
            self.forward_on_depth_two_sided,
            self.wireframe,
            self.edges,
            self.sky,
            self.transmittance,
            self.multi_scattering,
            self.sky_view,
            self.aerial_perspective,
            self.clouds,
            self.cloud_shadows,
            self.cloud_panorama,
            self.gtao,
            self.ao_denoise,
            self.ao_temporal,
            self.contact_shadows,
            self.reflections,
            self.fog,
            self.volume_scatter,
            self.volume_integrate,
            self.volumetric_fog,
            self.taa,
            self.taa_upscale,
            self.dof,
            self.motion_blur,
            self.bloom_down,
            self.bloom_up,
            self.exposure,
            self.glass_copy,
            self.foliage_push,
            self.tracks,
            self.particle_sim,
            self.erosion_flux,
            self.erosion_water,
            self.erosion_carve,
            self.erosion_carry,
            self.erosion_slump,
            self.erosion_settle,
            self.sky_light,
            self.glass,
            self.glass_shadow,
            self.glass_shadow_mip,
            self.ocean_rows,
            self.ocean_columns,
            self.ocean_mip,
            self.ocean_foam,
            self.ocean_mips,
            self.water_trails_carry,
            self.water_trails,
            self.caustics,
            self.floor_depth,
            self.foam_pattern,
            self.water_depth_copy,
            self.water,
            self.water_pool,
            self.water_stream,
            self.lava_spatter,
            self.lava_domes,
            self.sea_floor,
            self.sea_floor_under,
            self.sea_floor_walls,
            self.particles,
            self.particle_trails,
            self.particle_ribbon,
            self.particle_meshes,
            self.particle_composite,
            self.sea_spray,
            self.stream_spray,
            self.smoke_march,
            self.smoke_composite,
            self.smoke_resolve,
            self.dice_split,
            self.dice_patch,
            self.depth_diced,
            self.forward_diced,
            self.forward_on_depth_diced,
            self.skinning,
            self.shadow_diced,
            self.terrain_forward,
            self.terrain_depth,
            self.terrain_forward_on_depth,
            self.terrain_wireframe,
            self.terrain_shadow,
            self.terrain_local_shadow,
            self.block_forward,
            self.block_depth,
            self.block_forward_on_depth,
            self.voxel_coarse,
            self.voxel_probe,
            self.voxel_depth,
            self.voxel_forward,
            self.voxel_shadows,
            self.voxel_lamp_shadows,
            self.voxel_glass,
            self.voxel_light_cells,
            self.voxel_sun_map,
            self.voxel_light_sky,
            self.voxel_light_spread,
            self.voxel_world_columns,
            self.voxel_world_tops,
            self.voxel_world_trees,
            self.voxel_world_edits,
            self.voxel_world_candidates,
            self.voxel_world_arguments,
            self.voxel_world_coarse,
            self.voxel_world_brick_arguments,
            self.voxel_world_bricks,
            self.voxel_world_free,
            self.voxel_world_pack,
            self.voxel_world_sectors,
            self.voxel_world_places,
            self.block_wireframe,
            self.block_shadow,
            self.block_shadow_solid,
            self.block_leaf_light,
            self.block_local_shadow,
            self.grass_reset,
            self.grass_scatter,
            self.ground_cover_scatter,
            self.grass_clamp,
            self.grass_depth,
            self.grass_forward,
            self.grass_forward_on_depth,
            self.cover_depth,
            self.cover_forward,
            self.cover_forward_on_depth,
            self.dry_mesh,
            self.water_wall,
            self.dry_mesh_wall,
            self.block_water_exits,
            self.block_water,
            self.voxel_water,
            self.voxel_falls,
            self.voxel_drops,
            self.sea_floor_voxels,
            self.sea_floor_voxels_under,
            self.block_sea_inside,
            self.underwater,
            self.lens_wetness,
            self.composite_ldr,
            self.composite,
            self.finish,
            self.enlarge,
            self.background,
            self.clamp_light,
            self.frame_debug,
            self.ui,
            self.ui_panel,
            self.ui_panels,
            self.ui_panel_text,
            self.ui_panel_mips,
            self.debug,
            self.debug_solid,
            self.preview,
            self.preview_resolve,
            self.map,
            self.portal_surface,
            self.portal_mask,
            self.portal_mist,
            self.portal_depth,
            self.portal_depth_over,
            self.portal_depth_nearer,
            self.portal_light,
            self.impostor_bake,
        ];
        let mesh = self.mesh.iter().flat_map(|m| {
            [m.shadow, m.sun_pages, m.local_shadow, m.shadow_two_sided, m.local_shadow_two_sided, m.forward]
                .into_iter()
                .chain([m.depth, m.forward_on_depth, m.wireframe, m.edges])
        });
        let traced = self.ddgi.into_iter().flatten().chain(self.ddgi_software).chain(self.ray_reflections);
        let encode = self.encode.iter().map(|(_, pipeline)| *pipeline);
        let probes = [self.nearest_pyramid, self.probe_capture_software, self.probe_downsample, self.probe_filter];
        let sounds = [self.sound_occlusion_software, self.sound_reverb_software];
        let traced = traced.chain(self.probe_capture).chain(probes).chain(self.sound_occlusion).chain(self.sound_reverb).chain(sounds);
        if let Some(fsr3) = &self.fsr3 {
            fsr3.destroy(gpu);
        }
        let encode = encode.chain([self.fsr_exposure, self.fsr_copy]);
        let traced = traced.chain(self.light_bake.into_iter().flatten()).chain(self.light_bake_software);
        for pipeline in all.into_iter().chain(mesh).chain(traced).chain(encode) {
            unsafe { gpu.device.destroy_pipeline(pipeline, None) };
        }
    }
}

impl Pipelines {
    /// What fits a display image to a window in `format`.
    pub fn encoder(&self, format: vk::Format) -> Option<vk::Pipeline> {
        self.encode.iter().find(|(made, _)| *made == format).map(|(_, pipeline)| *pipeline)
    }
}

/// The shadow, forward, depth, wireframe and edges pipelines as `Pipelines::new`
/// makes them, with the task shader culling meshlets and mesh shaders
/// making them in place of the vertex shaders.
fn mesh_pipelines(
    maker: &Maker,
    [task, forward, shadow]: &[ShaderModule; 3],
    (scene_formats, surface_formats): (&[vk::Format], &[vk::Format]),
) -> Result<MeshPipelines> {
    let graphics = |desc: &GraphicsPipelineDesc| maker.graphics(desc);
    let task = (task, c"meshletTask");
    let forward = (forward, c"forwardMesh");
    Ok(MeshPipelines {
        shadow: graphics(&GraphicsPipelineDesc {
            depth_format: Some(DEPTH_FORMAT),
            depth_write: true,
            depth_compare: vk::CompareOp::LESS_OR_EQUAL,
            depth_bias: Some(SHADOW_BIAS),
            depth_clamp: true,
            ..GraphicsPipelineDesc::mesh("Shadows", task, (shadow, c"shadowMesh"), None)
        })?,
        sun_pages: graphics(&GraphicsPipelineDesc {
            front_face: vk::FrontFace::COUNTER_CLOCKWISE,
            depth_format: Some(DEPTH_FORMAT),
            depth_write: true,
            depth_compare: vk::CompareOp::LESS_OR_EQUAL,
            depth_bias: Some(SHADOW_BIAS),
            depth_clamp: true,
            ..GraphicsPipelineDesc::mesh("Sun shadow pages", task, (shadow, c"shadowMesh"), None)
        })?,
        local_shadow: graphics(&GraphicsPipelineDesc {
            depth_format: Some(DEPTH_FORMAT),
            depth_write: true,
            depth_compare: vk::CompareOp::LESS_OR_EQUAL,
            depth_bias: Some(SHADOW_BIAS),
            ..GraphicsPipelineDesc::mesh("Local shadows", task, (shadow, c"shadowMesh"), None)
        })?,
        shadow_two_sided: graphics(&GraphicsPipelineDesc {
            cull: vk::CullModeFlags::NONE,
            depth_format: Some(DEPTH_FORMAT),
            depth_write: true,
            depth_compare: vk::CompareOp::LESS_OR_EQUAL,
            depth_bias: Some(SHADOW_BIAS),
            depth_clamp: true,
            ..GraphicsPipelineDesc::mesh("Shadows (two-sided)", task, (shadow, c"shadowMesh"), None)
        })?,
        local_shadow_two_sided: graphics(&GraphicsPipelineDesc {
            cull: vk::CullModeFlags::NONE,
            depth_format: Some(DEPTH_FORMAT),
            depth_write: true,
            depth_compare: vk::CompareOp::LESS_OR_EQUAL,
            depth_bias: Some(SHADOW_BIAS),
            ..GraphicsPipelineDesc::mesh("Local shadows (two-sided)", task, (shadow, c"shadowMesh"), None)
        })?,
        forward: graphics(&GraphicsPipelineDesc {
            color_formats: scene_formats,
            depth_format: Some(DEPTH_FORMAT),
            depth_write: true,
            depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
            ..GraphicsPipelineDesc::mesh("Opaque", task, forward, Some(c"fragmentMain"))
        })?,
        depth: graphics(&GraphicsPipelineDesc {
            color_formats: surface_formats,
            depth_format: Some(DEPTH_FORMAT),
            depth_write: true,
            depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
            ..GraphicsPipelineDesc::mesh("Depth", task, forward, Some(c"depthFragment"))
        })?,
        forward_on_depth: graphics(&GraphicsPipelineDesc {
            color_formats: scene_formats,
            depth_format: Some(DEPTH_FORMAT),
            depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
            ..GraphicsPipelineDesc::mesh("Opaque", task, forward, Some(c"fragmentMain"))
        })?,
        wireframe: graphics(&GraphicsPipelineDesc {
            color_formats: scene_formats,
            depth_format: Some(DEPTH_FORMAT),
            depth_write: true,
            depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
            cull: vk::CullModeFlags::NONE,
            polygon_mode: vk::PolygonMode::LINE,
            ..GraphicsPipelineDesc::mesh("Wireframe", task, forward, Some(c"fragmentMain"))
        })?,
        edges: graphics(&GraphicsPipelineDesc {
            color_formats: &[HDR_FORMAT],
            depth_format: Some(DEPTH_FORMAT),
            depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
            cull: vk::CullModeFlags::NONE,
            polygon_mode: vk::PolygonMode::LINE,
            blend: Blend::Alpha,
            depth_bias: Some(EDGE_BIAS),
            ..GraphicsPipelineDesc::mesh("Edges", task, forward, Some(c"edgeFragment"))
        })?,
    })
}

/// The shader modules [`Pipelines::make`] makes pipelines of: every GPU's,
/// the mesh shaders' where it has them, and ray queries' where it can trace.
type Modules<'a> = (&'a [ShaderModule; 32], &'a Option<[ShaderModule; 3]>, &'a Option<ShaderModule>);

/// What [`Pipelines::make`] asks for each pipeline: first noting each down,
/// to make them all at once, then handing back what was made, in the order
/// they were asked for.
enum Maker {
    Noting(vk::PipelineLayout, RefCell<Vec<PipelineJob>>),
    Handing(RefCell<std::vec::IntoIter<(String, vk::Pipeline)>>),
}

impl Maker {
    fn graphics(&self, desc: &GraphicsPipelineDesc) -> Result<vk::Pipeline> {
        match self {
            Self::Noting(layout, jobs) => {
                jobs.borrow_mut().push(PipelineJob::graphics(*layout, desc));
                Ok(vk::Pipeline::null())
            }
            Self::Handing(_) => self.hand(desc.name),
        }
    }

    fn compute(&self, name: &str, module: &ShaderModule, entry: &CStr) -> Result<vk::Pipeline> {
        match self {
            Self::Noting(layout, jobs) => {
                jobs.borrow_mut().push(PipelineJob::compute(*layout, name, module, entry));
                Ok(vk::Pipeline::null())
            }
            Self::Handing(_) => self.hand(name),
        }
    }

    /// The next pipeline made, which must be the one `name` asks for.
    fn hand(&self, name: &str) -> Result<vk::Pipeline> {
        let Self::Handing(made) = self else { unreachable!("only handing hands pipelines back") };
        let (made_as, pipeline) = made.borrow_mut().next().context("more pipelines were asked for than were made")?;
        ensure!(made_as == name, "pipelines were asked for in another order: {name} where {made_as} was made");
        Ok(pipeline)
    }
}

/// How many threads make pipelines at once: as many as the machine has, or
/// `VOXEAR_PIPELINE_THREADS`, which 1 makes one at a time.
fn pipeline_threads() -> usize {
    let set = std::env::var("VOXEAR_PIPELINE_THREADS").ok().and_then(|threads| threads.parse().ok());
    set.unwrap_or_else(|| std::thread::available_parallelism().map_or(4, |threads| threads.get())).max(1)
}
