//! HDR display output on a real GPU, rendering offscreen into images as an
//! HDR10 or scRGB window's would be, read back as they are. Each test skips
//! itself (with a note) on machines without a usable Vulkan device.

use voxear_core::math::Vec3;
use voxear_ecs::World;
use voxear_render::{
    AntiAliasing, AntiAliasingMode, Camera, MaterialDesc, MeshRenderer, PostProcess, Primitive, Renderer,
    RendererSettings, Sky,
    display::{self, DisplaySettings, Encoding, HdrChoice},
};
use voxear_scene::Transform;

const FRAME: f32 = 1.0 / 60.0;
const SIZE: (u32, u32) = (320, 180);
const DISPLAY: DisplaySettings = DisplaySettings { hdr: HdrChoice::On, paper_white: 200.0, peak: 1000.0 };

fn renderer(encoding: Encoding) -> Option<Renderer> {
    let made = Renderer::new_offscreen_hdr(SIZE.0, SIZE.1, encoding, DISPLAY, &RendererSettings::default());
    match made {
        Ok(renderer) => Some(renderer),
        Err(error) => {
            eprintln!("skipping: no usable GPU ({error:#})");
            None
        }
    }
}

/// A sphere glowing `glow` on black, filling the middle of the picture,
/// with fixed exposure and nothing over the picture but the tonemapper.
fn glowing_sphere(renderer: &mut Renderer, glow: f32) -> World {
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 0.0, -3.0))));
    let sphere = renderer.primitive(Primitive::Sphere).unwrap();
    let material = MaterialDesc { base_color: [0.0, 0.0, 0.0, 1.0], emissive: [glow; 3], ..MaterialDesc::default() };
    let material = renderer.add_material(&material).unwrap();
    world.spawn((MeshRenderer::new(sphere, material), Transform::from_position(Vec3::ZERO)));
    world.insert_resource(Sky { ambient: 0.0, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, sharpness: 0.0, ..AntiAliasing::default() };
    let mut post = PostProcess { anti_aliasing, ..PostProcess::neutral() };
    post.ambient_occlusion.enabled = false;
    post.contact_shadows.enabled = false;
    post.reflections.enabled = false;
    world.insert_resource(post);
    world
}

/// The middle pixel's colour as light: linear, 1 at paper white for SDR
/// and in nits for HDR.
fn middle(renderer: &mut Renderer, encoding: Encoding, glow: f32) -> [f32; 3] {
    let world = glowing_sphere(renderer, glow);
    let mut time = 0.0;
    for _ in 0..4 {
        time += FRAME;
        renderer.render(&world, time).expect("frame renders");
    }
    let (width, height, pixels) = renderer.read_pixels().unwrap();
    let at = (height / 2 * width + width / 2) as usize;
    match encoding {
        Encoding::Sdr => {
            let srgb = |c: u8| {
                let c = f32::from(c) / 255.0;
                if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
            };
            [pixels[at * 4], pixels[at * 4 + 1], pixels[at * 4 + 2]].map(srgb)
        }
        Encoding::ScRgb => {
            let half = |i: usize| half_to_f32(u16::from_le_bytes([pixels[at * 8 + i * 2], pixels[at * 8 + i * 2 + 1]]));
            [half(0), half(1), half(2)].map(|c| c * 80.0)
        }
        Encoding::Hdr10 => {
            let packed = u32::from_le_bytes(pixels[at * 4..at * 4 + 4].try_into().unwrap());
            let channel = |shift: u32| ((packed >> shift) & 1023) as f32 / 1023.0;
            [channel(0), channel(10), channel(20)].map(|signal| display::pq_light(signal) * 10_000.0)
        }
    }
}

fn half_to_f32(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exponent = i32::from((bits >> 10) & 0x1f);
    let fraction = f32::from(bits & 0x3ff) / 1024.0;
    match exponent {
        0 => sign * fraction * 2f32.powi(-14),
        31 => sign * f32::INFINITY,
        _ => sign * (1.0 + fraction) * 2f32.powi(exponent - 15),
    }
}

#[test]
fn hdr_outputs_keep_the_midtones_at_paper_white_and_lift_the_highlights() {
    let (Some(mut sdr), Some(mut scrgb), Some(mut hdr10)) =
        (renderer(Encoding::Sdr), renderer(Encoding::ScRgb), renderer(Encoding::Hdr10))
    else {
        return;
    };
    assert_eq!(scrgb.display().1.encoding, Encoding::ScRgb);
    let paper = DISPLAY.paper_white;
    // A dim glow, below paper white: shown as SDR shows it, at paper white's brightness.
    let dim = middle(&mut sdr, Encoding::Sdr, 0.3);
    let (dim_scrgb, dim_hdr10) = (middle(&mut scrgb, Encoding::ScRgb, 0.3), middle(&mut hdr10, Encoding::Hdr10, 0.3));
    eprintln!("dim: SDR {dim:?}, scRGB {dim_scrgb:?} nits, HDR10 {dim_hdr10:?} nits");
    assert!(dim[0] > 0.05 && dim[0] < 0.9, "a midtone: {dim:?}");
    for (shown, what) in [(dim_scrgb, "scRGB"), (dim_hdr10, "HDR10")] {
        for (c, sdr) in shown.iter().zip(dim) {
            assert!((c - sdr * paper).abs() < sdr * paper * 0.04 + 1.5, "{what} midtones as SDR's: {shown:?}, {dim:?}");
        }
    }
    // A bright glow: white in SDR, far brighter than paper white in HDR, never past the peak.
    let bright = middle(&mut sdr, Encoding::Sdr, 40.0);
    let (bright_scrgb, bright_hdr10) =
        (middle(&mut scrgb, Encoding::ScRgb, 40.0), middle(&mut hdr10, Encoding::Hdr10, 40.0));
    eprintln!("bright: SDR {bright:?}, scRGB {bright_scrgb:?} nits, HDR10 {bright_hdr10:?} nits");
    assert!(bright.iter().all(|c| *c > 0.85), "near white in SDR: {bright:?}");
    let gain = display::highlight_gain(40.0, DISPLAY.peak / paper);
    for (shown, what) in [(bright_scrgb, "scRGB"), (bright_hdr10, "HDR10")] {
        for (c, sdr) in shown.iter().zip(bright) {
            assert!(*c > paper * 2.5 && *c <= DISPLAY.peak * 1.02, "{what} highlights towards the peak: {shown:?}");
            let expected = sdr * paper * gain;
            assert!((c - expected).abs() < expected * 0.05, "{what} as the roll-off has it: {c} for {expected}");
        }
    }
}

#[test]
fn a_window_without_hdr_beside_an_hdr_one_shows_sdr() {
    let (Some(mut plain), Some(mut hdr)) = (renderer(Encoding::Sdr), renderer(Encoding::Hdr10)) else { return };
    let alone = middle(&mut plain, Encoding::Sdr, 0.3);
    // The view drawn into a second, SDR, image of the HDR renderer: through a display image and the encode pass.
    let beside = hdr.add_offscreen_output(SIZE.0, SIZE.1).unwrap();
    hdr.set_view_output(beside);
    let world = glowing_sphere(&mut hdr, 0.3);
    let mut time = 0.0;
    for _ in 0..4 {
        time += FRAME;
        hdr.render(&world, time).expect("frame renders");
    }
    let (width, height, pixels) = hdr.read_output_pixels(beside).unwrap();
    let at = ((height / 2 * width + width / 2) * 4) as usize;
    let srgb = |c: u8| {
        let c = f32::from(c) / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    let shown = [pixels[at], pixels[at + 1], pixels[at + 2]].map(srgb);
    for (c, alone) in shown.iter().zip(alone) {
        assert!((c - alone).abs() < 0.02, "as SDR alone shows it: {shown:?} against {alone:?}");
    }
}

#[test]
fn an_image_renderer_reports_its_output_and_keeps_settings_held() {
    let Some(mut renderer) = renderer(Encoding::Sdr) else { return };
    let (settings, state) = renderer.display();
    assert_eq!(state.encoding, Encoding::Sdr);
    assert!(!state.offered.hdr10 && !state.offered.scrgb, "an SDR image offers no HDR");
    assert_eq!(settings.paper_white, 200.0);
    renderer.set_display(DisplaySettings { hdr: HdrChoice::On, paper_white: 20.0, peak: 50_000.0 }).unwrap();
    let (settings, state) = renderer.display();
    assert_eq!((settings.paper_white, settings.peak), (40.0, 10_000.0), "held to what displays show");
    assert_eq!(state.encoding, Encoding::Sdr, "with no HDR to be had, SDR");
}
