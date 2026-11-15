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
    sphere_of(
        renderer,
        MaterialDesc { base_color: [0.0, 0.0, 0.0, 1.0], emissive: [glow; 3], ..MaterialDesc::default() },
    )
}

/// A sphere of `material` on black, as `glowing_sphere`.
fn sphere_of(renderer: &mut Renderer, material: MaterialDesc) -> World {
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 0.0, -3.0))));
    let sphere = renderer.primitive(Primitive::Sphere).unwrap();
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

/// A Radiance HDR file of one colour, `light` past white.
fn radiance(light: [f32; 3]) -> Vec<u8> {
    let picture = image::Rgb32FImage::from_fn(16, 16, |_, _| image::Rgb(light));
    let mut file = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb32F(picture).write_to(&mut file, image::ImageFormat::Hdr).unwrap();
    file.into_inner()
}

/// The middle of a sphere whose glow map is `texture`, in nits, shown as scRGB.
fn glow_mapped(renderer: &mut Renderer, texture: voxear_rhi::TextureId) -> [f32; 3] {
    let glowing = MaterialDesc {
        base_color: [0.0, 0.0, 0.0, 1.0],
        emissive: [1.0; 3],
        emissive_texture: Some(texture),
        ..MaterialDesc::default()
    };
    let world = sphere_of(renderer, glowing);
    let mut time = 0.0;
    for _ in 0..4 {
        time += FRAME;
        renderer.render(&world, time).expect("frame renders");
    }
    let (width, height, pixels) = renderer.read_pixels().unwrap();
    let at = (height / 2 * width + width / 2) as usize;
    let half = |i: usize| half_to_f32(u16::from_le_bytes([pixels[at * 8 + i * 2], pixels[at * 8 + i * 2 + 1]]));
    [half(0), half(1), half(2)].map(|c| c * 80.0)
}

#[test]
fn an_hdr_glow_map_glows_past_white_cooked_or_not() {
    use voxear_assets::TextureImport;
    use voxear_render::{CookedPicture, PictureUse};
    let Some(mut renderer) = renderer(Encoding::ScRgb) else { return };
    let file = radiance([0.2, 3.0, 12.0]);
    let compressed = CookedPicture::cook(&file, &TextureImport::default(), PictureUse::Colour).unwrap();
    let kept = TextureImport { compress: false, ..TextureImport::default() };
    let halves = CookedPicture::cook(&file, &kept, PictureUse::Colour).unwrap();
    assert!(compressed.is_hdr() && halves.half_floats().is_some());
    let bc6h = renderer.add_cooked_picture("bc6h", &compressed, true).unwrap();
    let half = renderer.add_cooked_picture("half floats", &halves, true).unwrap();
    let (a, b) = (glow_mapped(&mut renderer, bc6h), glow_mapped(&mut renderer, half));
    eprintln!("BC6H {a:?} nits, half floats {b:?} nits");
    assert!(b[2] > DISPLAY.paper_white && b[2] > b[1] && b[1] > b[0], "the map's light past white: {b:?}");
    for c in 0..3 {
        assert!((a[c] - b[c]).abs() <= b[c] * 0.06 + 1.0, "BC6H as the half floats are: {a:?} against {b:?}");
    }
}

/// What the middle of the view shows of a sky drawn from a panorama whose
/// left half is red and right half a bright green, turned by `turn` degrees,
/// in nits, shown as scRGB.
fn picture_sky(renderer: &mut Renderer, turn: f32) -> [f32; 3] {
    let (width, height) = (64u32, 32u32);
    let pixels: Vec<[f32; 4]> = (0..width * height)
        .map(|i| if i % width < width / 2 { [1.0, 0.0, 0.0, 1.0] } else { [0.0, 4.0, 0.0, 1.0] })
        .collect();
    let texture = renderer.add_hdr_texture("sky", (width, height), &pixels).unwrap();
    let light = voxear_render::picture_sky_light(&pixels, width, height);
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::ZERO)));
    let sky = Sky { picture_turn: turn, picture_texture: Some(texture), picture_light: Some(light), ..Sky::default() };
    world.insert_resource(sky);
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, sharpness: 0.0, ..AntiAliasing::default() };
    let mut post = PostProcess { anti_aliasing, ..PostProcess::neutral() };
    post.fog.enabled = false;
    world.insert_resource(post);
    let mut time = 0.0;
    for _ in 0..4 {
        time += FRAME;
        renderer.render(&world, time).expect("frame renders");
    }
    let (width, height, pixels) = renderer.read_pixels().unwrap();
    let at = (height / 2 * width + width / 2) as usize;
    let half = |i: usize| half_to_f32(u16::from_le_bytes([pixels[at * 8 + i * 2], pixels[at * 8 + i * 2 + 1]]));
    [half(0), half(1), half(2)].map(|c| c * 80.0)
}

#[test]
fn a_sky_is_drawn_from_its_picture_turned_as_it_says() {
    let Some(mut renderer) = renderer(Encoding::ScRgb) else { return };
    let (one_way, other_way) = (picture_sky(&mut renderer, 90.0), picture_sky(&mut renderer, -90.0));
    eprintln!("turned 90°: {one_way:?} nits, -90°: {other_way:?} nits");
    assert!(one_way[1] > one_way[0] * 2.0, "the green half ahead: {one_way:?}");
    assert!(one_way[1] > DISPLAY.paper_white, "as bright as the picture is, past white: {one_way:?}");
    assert!(other_way[0] > other_way[1] * 2.0, "the red half ahead: {other_way:?}");
}
