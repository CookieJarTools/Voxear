//! Box models: characters and things built of boxes, each box with pixels
//! on its faces, grouped under bones. A `.boxmodel` file is a model as a
//! `.glb` is one: [`prepare`] reads it into the same [`PreparedModel`], one
//! skinned mesh with every box bound whole to its bone, the bones as its
//! skeleton and its pixels as its one texture, so whatever draws, poses or
//! ships a model does a box model too.
//!
//! Everything is measured in the model's own pixels: `pixels_per_block` of
//! them make a metre, a block of a world of blocks at its usual size. A
//! bone's pivot is measured from its parent's, and a box from its bone's
//! pivot, each in the space of what it hangs from, so moving a bone moves
//! all under it. The model looks along +Z: its front faces are those at +Z,
//! its right is +X.
//!
//! The file is TOML: the sheet's size and pixels, then the bones, each with
//! its boxes.
//!
//! ```toml
//! pixels_per_block = 16
//! sheet = [64, 32]
//! pixels = "…"
//!
//! [[bones]]
//! name = "Body"
//! pivot = [0.0, 12.0, 0.0]
//!
//! [[bones.boxes]]
//! from = [-4.0, 0.0, -2.0]
//! size = [8, 12, 4]
//! uv = [16, 16]
//!
//! [[bones]]
//! name = "Head"
//! parent = "Body"
//! pivot = [0.0, 12.0, 0.0]
//! ```
//!
//! The pixels are kept small: the sheet's colours as a palette and a byte a
//! pixel where it has 256 or fewer, compressed, as text (see [`Sheet`]).

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use voxear_core::math::{EulerRot, Mat4, Quat, Vec3};

use crate::{
    assets::MaterialDesc,
    gltf_import::{PreparedMaterial, PreparedModel, PreparedNode, PreparedTexture, TextureFormat},
    lod::full_lod,
    mesh::{MeshData, Vertex},
    mesh_tools,
    skeleton::{Bone, BonePose, ModelAnimation, Skeleton, Skin, SkinVertex},
};

mod edit;
mod paint;
mod rig;

pub use edit::BoxHit;

/// A box model's file extension.
pub const EXTENSION: &str = "boxmodel";

/// How many of a model's pixels make a metre unless it says otherwise: a
/// block's side as blocks are usually drawn.
pub const PIXELS_PER_BLOCK: f32 = 16.0;

/// The most bones a model has, and the longest side of its sheet in pixels.
pub const MOST_BONES: usize = 256;
pub const LARGEST_SHEET: u32 = 1024;

/// How far inside its own pixels a face's corners sit, in pixels, so the
/// pixel just past its edge is never the one drawn.
const INSET: f32 = 1.0 / 64.0;

/// Whether `path` names a box model.
pub fn is_box_model(path: &str) -> bool {
    std::path::Path::new(path).extension().is_some_and(|extension| extension.eq_ignore_ascii_case(EXTENSION))
}

/// A box's six faces, by the way each looks out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Face {
    /// +X.
    Right,
    /// -X.
    Left,
    /// +Y.
    Top,
    /// -Y.
    Bottom,
    /// +Z.
    Front,
    /// -Z.
    Back,
}

impl Face {
    pub const ALL: [Face; 6] = [Face::Right, Face::Left, Face::Top, Face::Bottom, Face::Front, Face::Back];

    /// The way it looks out.
    pub fn normal(self) -> Vec3 {
        match self {
            Face::Right => Vec3::X,
            Face::Left => Vec3::NEG_X,
            Face::Top => Vec3::Y,
            Face::Bottom => Vec3::NEG_Y,
            Face::Front => Vec3::Z,
            Face::Back => Vec3::NEG_Z,
        }
    }

    /// Which way its pixels run across the sheet and down it, seen from
    /// outside the box: the sides upright and going round the box as they
    /// go along the sheet, the top's lower edge the front's, the bottom's
    /// upper edge the front's.
    pub fn across_and_down(self) -> (Vec3, Vec3) {
        match self {
            Face::Right => (Vec3::Z, Vec3::NEG_Y),
            Face::Left => (Vec3::NEG_Z, Vec3::NEG_Y),
            Face::Top => (Vec3::NEG_X, Vec3::Z),
            Face::Bottom => (Vec3::NEG_X, Vec3::NEG_Z),
            Face::Front => (Vec3::NEG_X, Vec3::NEG_Y),
            Face::Back => (Vec3::X, Vec3::NEG_Y),
        }
    }

    /// Its place among [`Face::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }
}

/// Where on the sheet a face's pixels are: its left, top, right and bottom
/// edges in pixels. Right before left, or bottom before top, draws them
/// turned over.
pub type FaceRect = [f32; 4];

/// Where a box's faces find their pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BoxSkin {
    /// Laid out as the box unfolded, its upper left corner at `at`: the top
    /// and bottom side by side over the front, then the right, front, left
    /// and back in a row. `mirror` turns the layout over from left to
    /// right, for a limb sharing its pixels with the one across the body.
    Unfolded { at: [u32; 2], mirror: bool },
    /// Each face its own place, by [`Face::index`]; a face with none is
    /// not drawn.
    Faces([Option<FaceRect>; 6]),
}

/// One box of a model.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelBox {
    /// Its least corner, in pixels from its bone's pivot.
    pub from: Vec3,
    /// How many pixels it measures along X, Y and Z. A side of none makes
    /// it a flat sheet, as hair and cloth are.
    pub size: [u32; 3],
    /// How it is turned about its middle, in degrees.
    pub rotation: Vec3,
    /// How far each face stands out past its size, in pixels, its pixels
    /// stretched to fit: for clothes over a body.
    pub inflate: f32,
    pub skin: BoxSkin,
}

impl ModelBox {
    /// A box of `size` pixels from `from`, unfolded on the sheet at `uv`.
    pub fn new(from: Vec3, size: [u32; 3], uv: [u32; 2]) -> Self {
        Self { from, size, rotation: Vec3::ZERO, inflate: 0.0, skin: BoxSkin::Unfolded { at: uv, mirror: false } }
    }

    /// Where `face`'s pixels are on the sheet, or `None` for a face that is
    /// not drawn.
    pub fn face_rect(&self, face: Face) -> Option<FaceRect> {
        let (at, mirror) = match self.skin {
            BoxSkin::Faces(faces) => return faces[face.index()],
            BoxSkin::Unfolded { at, mirror } => (at, mirror),
        };
        let [w, h, d] = self.size.map(|side| side as f32);
        let (u, v) = (at[0] as f32, at[1] as f32);
        // A mirrored box's sides change places, and each face runs the other way.
        let face = match (mirror, face) {
            (true, Face::Right) => Face::Left,
            (true, Face::Left) => Face::Right,
            (_, face) => face,
        };
        let [left, top, right, bottom] = match face {
            Face::Top => [u + d, v, u + d + w, v + d],
            Face::Bottom => [u + d + w, v, u + d + w + w, v + d],
            Face::Right => [u, v + d, u + d, v + d + h],
            Face::Front => [u + d, v + d, u + d + w, v + d + h],
            Face::Left => [u + d + w, v + d, u + d + w + d, v + d + h],
            Face::Back => [u + d + w + d, v + d, u + d + w + d + w, v + d + h],
        };
        Some(if mirror { [right, top, left, bottom] } else { [left, top, right, bottom] })
    }

    /// How many pixels of the sheet it takes unfolded, across and down.
    pub fn unfolded_size(&self) -> [u32; 2] {
        let [w, h, d] = self.size;
        [2 * (w + d), d + h]
    }
}

/// What part of a body a bone moves, for what animates the model to know
/// its legs from its arms without going by their names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BoneRole {
    /// Nothing in particular.
    #[default]
    None,
    Body,
    Head,
    /// A leg, or under a leg its lower half.
    Leg,
    /// An arm, or under an arm its lower half.
    Arm,
    Tail,
    Wing,
}

impl BoneRole {
    pub const ALL: [BoneRole; 7] =
        [BoneRole::None, BoneRole::Body, BoneRole::Head, BoneRole::Leg, BoneRole::Arm, BoneRole::Tail, BoneRole::Wing];

    /// As a list offers it.
    pub fn label(self) -> &'static str {
        match self {
            BoneRole::None => "None",
            BoneRole::Body => "Body",
            BoneRole::Head => "Head",
            BoneRole::Leg => "Leg",
            BoneRole::Arm => "Arm",
            BoneRole::Tail => "Tail",
            BoneRole::Wing => "Wing",
        }
    }
}

/// A bone and the boxes it carries.
#[derive(Clone, Debug, PartialEq)]
pub struct BoxBone {
    pub name: String,
    /// The part of the body it moves.
    pub role: BoneRole,
    /// Its parent, always before it among the model's bones.
    pub parent: Option<usize>,
    /// The point it turns about, in pixels from its parent's pivot, or from
    /// the model's feet for a bone with no parent.
    pub pivot: Vec3,
    /// How it stands when nothing poses it, in degrees.
    pub rotation: Vec3,
    pub boxes: Vec<ModelBox>,
}

impl BoxBone {
    pub fn new(name: &str, parent: Option<usize>, pivot: Vec3) -> Self {
        Self { name: name.to_string(), role: BoneRole::None, parent, pivot, rotation: Vec3::ZERO, boxes: Vec::new() }
    }
}

/// A model's pixels, four bytes each, red, green, blue and how solid, row
/// by row from the top. A pixel less than half solid is a hole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sheet {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Sheet {
    /// A sheet of `width` × `height` pixels, all `color`.
    pub fn filled(width: u32, height: u32, color: [u8; 4]) -> Self {
        Self { width, height, rgba: color.repeat((width * height) as usize) }
    }

    pub fn get(&self, x: u32, y: u32) -> [u8; 4] {
        if x >= self.width || y >= self.height {
            return [0; 4];
        }
        let at = ((y * self.width + x) * 4) as usize;
        self.rgba.get(at..at + 4).map_or([0; 4], |pixel| [pixel[0], pixel[1], pixel[2], pixel[3]])
    }

    /// Colours a pixel; one off the sheet is left.
    pub fn set(&mut self, x: u32, y: u32, color: [u8; 4]) {
        if x < self.width && y < self.height {
            let at = ((y * self.width + x) * 4) as usize;
            self.rgba[at..at + 4].copy_from_slice(&color);
        }
    }

    /// Colours every pixel of a rectangle that lies on the sheet.
    pub fn fill(&mut self, [left, top]: [u32; 2], [width, height]: [u32; 2], color: [u8; 4]) {
        for y in top..(top + height).min(self.height) {
            for x in left..(left + width).min(self.width) {
                self.set(x, y, color);
            }
        }
    }

    /// The pixels as the file keeps them: where the sheet has 256 colours
    /// or fewer, a 1, how many colours less one, the colours and then a
    /// byte a pixel; else a 0 and the pixels as they are. Compressed, and
    /// written as base64.
    pub fn to_text(&self) -> String {
        let mut colors: Vec<[u8; 4]> = Vec::new();
        let mut places: Vec<u8> = Vec::with_capacity(self.rgba.len() / 4);
        for pixel in self.rgba.chunks_exact(4) {
            let pixel = [pixel[0], pixel[1], pixel[2], pixel[3]];
            let place = colors.iter().position(|&color| color == pixel).unwrap_or_else(|| {
                colors.push(pixel);
                colors.len() - 1
            });
            if place > 255 {
                break;
            }
            places.push(place as u8);
        }
        let mut body = Vec::new();
        if colors.len() <= 256 && !colors.is_empty() {
            body.push(1);
            body.push((colors.len() - 1) as u8);
            body.extend(colors.iter().flatten());
            body.extend(places);
        } else {
            body.push(0);
            body.extend(&self.rgba);
        }
        base64::encode(lz4_flex::compress_prepend_size(&body))
    }

    /// A sheet of `width` × `height` pixels from [`to_text`](Self::to_text)'s text.
    pub fn from_text(width: u32, height: u32, text: &str) -> Result<Self> {
        let packed = base64::decode(text.trim()).context("the pixels are not base64")?;
        let body = lz4_flex::decompress_size_prepended(&packed).context("the pixels could not be unpacked")?;
        let count = (width * height) as usize;
        let rgba = match body.split_first() {
            Some((0, pixels)) => pixels.to_vec(),
            Some((1, rest)) => {
                let (&most, rest) = rest.split_first().context("the pixels have no colours")?;
                let colors = usize::from(most) + 1;
                ensure!(rest.len() >= colors * 4, "the pixels' colours are cut short");
                let (palette, places) = rest.split_at(colors * 4);
                let mut rgba = Vec::with_capacity(count * 4);
                for &place in places {
                    let at = usize::from(place) * 4;
                    rgba.extend(palette.get(at..at + 4).context("a pixel of a colour the sheet has not")?);
                }
                rgba
            }
            _ => bail!("the pixels are kept in a way not known"),
        };
        ensure!(rgba.len() == count * 4, "{} pixels for a sheet of {width} × {height}", rgba.len() / 4);
        Ok(Self { width, height, rgba })
    }
}

/// A model of boxes under bones, with its pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct BoxModel {
    /// How many of its pixels make a metre.
    pub pixels_per_block: f32,
    pub sheet: Sheet,
    /// Its bones, every parent before its children.
    pub bones: Vec<BoxBone>,
}

impl Default for BoxModel {
    fn default() -> Self {
        Self { pixels_per_block: PIXELS_PER_BLOCK, sheet: Sheet::filled(64, 64, [0; 4]), bones: Vec::new() }
    }
}

/// The model as its file has it.
#[derive(Serialize, Deserialize)]
struct FileModel {
    #[serde(default = "pixels_per_block")]
    pixels_per_block: f32,
    sheet: [u32; 2],
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pixels: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    bones: Vec<FileBone>,
}

fn pixels_per_block() -> f32 {
    PIXELS_PER_BLOCK
}

#[derive(Serialize, Deserialize)]
struct FileBone {
    name: String,
    #[serde(default, skip_serializing_if = "is_no_role")]
    role: BoneRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parent: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pivot: [f32; 3],
    #[serde(default, skip_serializing_if = "is_zero")]
    rotation: [f32; 3],
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    boxes: Vec<FileBox>,
}

#[derive(Serialize, Deserialize)]
struct FileBox {
    from: [f32; 3],
    size: [u32; 3],
    #[serde(default, skip_serializing_if = "is_zero")]
    rotation: [f32; 3],
    #[serde(default, skip_serializing_if = "is_none")]
    inflate: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    uv: Option<[u32; 2]>,
    #[serde(default, skip_serializing_if = "is_false")]
    mirror: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    faces: Option<FileFaces>,
}

#[derive(Default, Serialize, Deserialize)]
struct FileFaces {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    right: Option<FaceRect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    left: Option<FaceRect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    top: Option<FaceRect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bottom: Option<FaceRect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    front: Option<FaceRect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    back: Option<FaceRect>,
}

fn is_zero(value: &[f32; 3]) -> bool {
    *value == [0.0; 3]
}

fn is_none(value: &f32) -> bool {
    *value == 0.0
}

fn is_no_role(role: &BoneRole) -> bool {
    *role == BoneRole::None
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl BoxModel {
    /// Reads a model from its file's text.
    pub fn parse(text: &str) -> Result<Self> {
        let file: FileModel = toml::from_str(text)?;
        let [width, height] = file.sheet;
        ensure!(
            (1..=LARGEST_SHEET).contains(&width) && (1..=LARGEST_SHEET).contains(&height),
            "a sheet of {width} × {height} pixels"
        );
        ensure!(file.pixels_per_block > 0.0, "{} pixels to a block", file.pixels_per_block);
        ensure!(file.bones.len() <= MOST_BONES, "{} bones, more than {MOST_BONES}", file.bones.len());
        let sheet = match file.pixels.is_empty() {
            true => Sheet::filled(width, height, [255; 4]),
            false => Sheet::from_text(width, height, &file.pixels)?,
        };
        let mut bones: Vec<BoxBone> = Vec::with_capacity(file.bones.len());
        for bone in file.bones {
            ensure!(!bone.name.is_empty(), "a bone with no name");
            ensure!(bones.iter().all(|other| other.name != bone.name), "two bones named {}", bone.name);
            let parent = match &bone.parent {
                Some(parent) => Some(
                    bones
                        .iter()
                        .position(|other| other.name == *parent)
                        .with_context(|| format!("{}'s parent {parent} is not a bone before it", bone.name))?,
                ),
                None => None,
            };
            let boxes = bone.boxes.into_iter().map(read_box).collect::<Result<_>>();
            bones.push(BoxBone {
                parent,
                role: bone.role,
                pivot: bone.pivot.into(),
                rotation: bone.rotation.into(),
                boxes: boxes.with_context(|| format!("reading {}'s boxes", bone.name))?,
                name: bone.name,
            });
        }
        Ok(Self { pixels_per_block: file.pixels_per_block, sheet, bones })
    }

    /// The model as its file's text.
    pub fn to_text(&self) -> String {
        let file = FileModel {
            pixels_per_block: self.pixels_per_block,
            sheet: [self.sheet.width, self.sheet.height],
            pixels: self.sheet.to_text(),
            bones: self
                .bones
                .iter()
                .map(|bone| FileBone {
                    name: bone.name.clone(),
                    role: bone.role,
                    parent: bone.parent.and_then(|parent| Some(self.bones.get(parent)?.name.clone())),
                    pivot: bone.pivot.into(),
                    rotation: bone.rotation.into(),
                    boxes: bone.boxes.iter().map(write_box).collect(),
                })
                .collect(),
        };
        HEADING.to_string() + &toml::to_string(&file).expect("a box model is plain values")
    }

    /// Adds a bone and returns its place among the bones.
    pub fn add_bone(&mut self, name: &str, parent: Option<usize>, pivot: Vec3) -> usize {
        self.bones.push(BoxBone::new(name, parent, pivot));
        self.bones.len() - 1
    }

    /// Adds a box of `size` pixels to a bone, its least corner `from` the
    /// bone's pivot, unfolded on the sheet at `uv`.
    pub fn add_box(&mut self, bone: usize, from: Vec3, size: [u32; 3], uv: [u32; 2]) -> &mut ModelBox {
        let boxes = &mut self.bones[bone].boxes;
        boxes.push(ModelBox::new(from, size, uv));
        boxes.last_mut().expect("just added")
    }

    pub fn find_bone(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|bone| bone.name == name)
    }

    /// How many metres one of its pixels is.
    pub fn pixel_size(&self) -> f32 {
        1.0 / self.pixels_per_block.max(f32::EPSILON)
    }

    /// Its bones as a skeleton, each at rest where its pivot and rotation
    /// put it, in metres.
    pub fn skeleton(&self) -> Skeleton {
        let size = self.pixel_size();
        let bones = self.bones.iter().map(|bone| Bone {
            name: bone.name.clone(),
            parent: bone.parent,
            rest: BonePose { translation: bone.pivot * size, rotation: turned(bone.rotation), scale: Vec3::ONE },
        });
        Skeleton { bones: bones.collect() }
    }

    /// Its boxes as one mesh in the model's space, in metres, as the bones
    /// stand at rest, each box's corners bound whole to its bone.
    pub fn mesh(&self) -> MeshData {
        self.mesh_posed(&self.rest_matrices())
    }

    /// Each bone's place in the model's space as it stands at rest, in
    /// metres.
    pub fn rest_matrices(&self) -> Vec<Mat4> {
        let skeleton = self.skeleton();
        skeleton.model_matrices(&skeleton.rest())
    }

    /// Where a box is in the model's space, in metres, with its bone at
    /// `bone`: its middle and how it is turned, and how far its faces
    /// stand from its middle along its own X, Y and Z.
    pub fn box_place(&self, bone: &Mat4, model_box: &ModelBox) -> (Mat4, Vec3) {
        let size = self.pixel_size();
        let half = Vec3::from(model_box.size.map(|side| side as f32)) * 0.5;
        let in_bone = Mat4::from_rotation_translation(turned(model_box.rotation), (model_box.from + half) * size);
        (*bone * in_bone, (half + Vec3::splat(model_box.inflate)) * size)
    }

    /// [`mesh`](Self::mesh) with each bone where `bones` puts it in the
    /// model's space, one for each bone: the model posed without skinning,
    /// as its boxes move whole.
    pub fn mesh_posed(&self, bones: &[Mat4]) -> MeshData {
        let sheet = Vec3::new(self.sheet.width as f32, self.sheet.height as f32, 1.0);
        let mut data = MeshData::default();
        for ((bone, joint), in_model) in self.bones.iter().zip(0u16..).zip(bones) {
            for model_box in &bone.boxes {
                let (place, reach) = self.box_place(in_model, model_box);
                for face in Face::ALL {
                    let Some(rect) = model_box.face_rect(face) else { continue };
                    let (normal, (across, down)) = (face.normal(), face.across_and_down());
                    let (out, along, fall) = (normal * reach, across * reach, down * reach);
                    // A flat box's edges have no face to them.
                    if along.length_squared() == 0.0 || fall.length_squared() == 0.0 {
                        continue;
                    }
                    let [left, top, right, bottom] = inset(rect);
                    let corners = [
                        (out - along - fall, [left, top]),
                        (out + along - fall, [right, top]),
                        (out + along + fall, [right, bottom]),
                        (out - along + fall, [left, bottom]),
                    ];
                    let first = data.vertices.len() as u32;
                    let normal = place.transform_vector3(normal).normalize_or_zero();
                    for (corner, [u, v]) in corners {
                        data.vertices.push(Vertex {
                            position: place.transform_point3(corner).into(),
                            normal: normal.into(),
                            uv: [u / sheet.x, v / sheet.y],
                            ..Vertex::default()
                        });
                        data.skin.push(SkinVertex { joints: [joint, 0, 0, 0], weights: [u16::MAX, 0, 0, 0] });
                    }
                    data.indices.extend([first, first + 1, first + 2, first, first + 2, first + 3]);
                }
            }
        }
        mesh_tools::calculate_tangents(&mut data);
        data
    }

    /// The model ready to upload, as a model file read from `path` is: its
    /// boxes one skinned part, its sheet the part's material, its bones the
    /// skeleton. A model with no boxes has its bones alone.
    pub fn prepared(&self, path: &str) -> PreparedModel {
        let skeleton = self.skeleton();
        let inverse_binds = skeleton.model_matrices(&skeleton.rest()).iter().map(Mat4::inverse).collect();
        let skin = Skin { joints: (0..skeleton.bones.len()).collect(), inverse_binds, bounds: Vec::new() };
        let mut model = PreparedModel {
            path: path.to_string(),
            meshes: Vec::new(),
            textures: Vec::new(),
            materials: Vec::new(),
            nodes: Vec::new(),
            animation: Some(ModelAnimation { skeleton, skins: vec![skin], clips: Vec::new() }),
            roles: self.bones.iter().map(|bone| bone.role).collect(),
        };
        let data = self.mesh();
        if data.indices.is_empty() {
            return model;
        }
        let lods = full_lod(&data);
        model.meshes.push((data, lods));
        model.textures.push(PreparedTexture {
            name: SHEET.to_string(),
            width: self.sheet.width,
            height: self.sheet.height,
            srgb: true,
            format: TextureFormat::Rgba8,
            mips: vec![self.sheet.rgba.clone()],
            compress: false,
            source: None,
        });
        let desc = MaterialDesc { roughness: 1.0, alpha_cutoff: Some(0.5), pixels: true, ..MaterialDesc::default() };
        model.materials.push(PreparedMaterial {
            name: SHEET.to_string(),
            desc,
            base_color: Some(0),
            metallic_roughness: None,
            normal: None,
            emissive: None,
        });
        let name = std::path::Path::new(path).file_stem().map(|stem| stem.to_string_lossy().into_owned());
        model.nodes.push(PreparedNode {
            name: name.unwrap_or_default(),
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
            mesh: 0,
            material: Some(0),
            mesh_part: "mesh0.0".to_string(),
            material_part: Some("material0".to_string()),
            skin: Some(0),
        });
        model
    }

    /// A model to start from: one bone holding a block-sized box standing
    /// on the ground, its faces in two greys with a darker rim, so each
    /// face and its pixels can be told apart.
    pub fn starter() -> Self {
        let mut model = Self { sheet: Sheet::filled(64, 32, [0; 4]), ..Self::default() };
        let body = model.add_bone("Body", None, Vec3::ZERO);
        let side = PIXELS_PER_BLOCK as u32;
        let added = model.add_box(body, Vec3::new(-8.0, 0.0, -8.0), [side; 3], [0, 0]).clone();
        for face in Face::ALL {
            let Some([left, top, _, _]) = added.face_rect(face) else { continue };
            let light = if matches!(face, Face::Top | Face::Front) { 214 } else { 188 };
            model.sheet.fill([left as u32, top as u32], [side, side], [light - 40, light - 40, light - 40, 255]);
            model.sheet.fill([left as u32 + 1, top as u32 + 1], [side - 2, side - 2], [light, light, light, 255]);
        }
        model
    }
}

/// What a box model's file starts with.
const HEADING: &str = "# Voxear box model: boxes under bones, measured in the pixels of its sheet.\n";

/// What the model's one texture and material are called.
const SHEET: &str = "Sheet";

/// Degrees about X, Y and Z as a rotation, turned as an Obj's are.
pub(crate) fn turned(degrees: Vec3) -> Quat {
    Quat::from_euler(EulerRot::YXZ, degrees.y.to_radians(), degrees.x.to_radians(), degrees.z.to_radians())
}

/// A rotation as degrees about X, Y and Z, as [`turned`] reads them.
pub(crate) fn degrees(rotation: Quat) -> Vec3 {
    let (y, x, z) = rotation.to_euler(EulerRot::YXZ);
    let whole = |radians: f32| (radians.to_degrees() * 1000.0).round() / 1000.0 + 0.0;
    Vec3::new(whole(x), whole(y), whole(z))
}

/// A face's place on the sheet drawn in a little from each edge.
fn inset([left, top, right, bottom]: FaceRect) -> FaceRect {
    let toward = |from: f32, to: f32| from + (to - from).signum() * INSET.min((to - from).abs() * 0.5);
    [toward(left, right), toward(top, bottom), toward(right, left), toward(bottom, top)]
}

fn read_box(file: FileBox) -> Result<ModelBox> {
    let skin = match (file.uv, file.faces) {
        (Some(_), Some(_)) => bail!("a box with both uv and faces"),
        (None, Some(faces)) => {
            BoxSkin::Faces([faces.right, faces.left, faces.top, faces.bottom, faces.front, faces.back])
        }
        (uv, None) => BoxSkin::Unfolded { at: uv.unwrap_or_default(), mirror: file.mirror },
    };
    Ok(ModelBox {
        from: file.from.into(),
        size: file.size,
        rotation: file.rotation.into(),
        inflate: file.inflate,
        skin,
    })
}

fn write_box(model_box: &ModelBox) -> FileBox {
    let (uv, mirror, faces) = match model_box.skin {
        BoxSkin::Unfolded { at, mirror } => (Some(at), mirror, None),
        BoxSkin::Faces([right, left, top, bottom, front, back]) => {
            (None, false, Some(FileFaces { right, left, top, bottom, front, back }))
        }
    };
    FileBox {
        from: model_box.from.into(),
        size: model_box.size,
        rotation: model_box.rotation.into(),
        inflate: model_box.inflate,
        uv,
        mirror,
        faces,
    }
}

/// Reads a box model's file into a model ready to upload.
pub(crate) fn prepare(path: &str, read: &mut crate::gltf_import::ReadFile) -> Result<PreparedModel> {
    let bytes = read(path)?;
    let text = std::str::from_utf8(&bytes).context("the file is not text")?;
    Ok(BoxModel::parse(text)?.prepared(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A body with a head on it and an arm each side, the left arm's pixels
    /// the right's turned over.
    fn figure() -> BoxModel {
        let mut model = BoxModel::default();
        let body = model.add_bone("Body", None, Vec3::new(0.0, 12.0, 0.0));
        model.add_box(body, Vec3::new(-4.0, 0.0, -2.0), [8, 12, 4], [16, 16]);
        let head = model.add_bone("Head", Some(body), Vec3::new(0.0, 12.0, 0.0));
        model.add_box(head, Vec3::new(-4.0, 0.0, -4.0), [8, 8, 8], [0, 0]);
        let right = model.add_bone("RightArm", Some(body), Vec3::new(6.0, 10.0, 0.0));
        model.add_box(right, Vec3::new(-2.0, -10.0, -2.0), [4, 12, 4], [40, 16]);
        let left = model.add_bone("LeftArm", Some(body), Vec3::new(-6.0, 10.0, 0.0));
        model.add_box(left, Vec3::new(-2.0, -10.0, -2.0), [4, 12, 4], [40, 16]).skin =
            BoxSkin::Unfolded { at: [40, 16], mirror: true };
        model
    }

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-4
    }

    #[test]
    fn a_model_reads_back_as_it_was_written() {
        let mut model = figure();
        model.sheet.set(3, 5, [200, 40, 40, 255]);
        model.bones[1].rotation = Vec3::new(0.0, 15.0, 0.0);
        model.bones[2].boxes[0].inflate = 0.25;
        model.bones[2].boxes[0].rotation = Vec3::new(0.0, 0.0, 10.0);
        let mut faces = [None; 6];
        faces[Face::Front.index()] = Some([8.0, 8.0, 16.0, 16.0]);
        model.bones[1]
            .boxes
            .push(ModelBox { skin: BoxSkin::Faces(faces), ..ModelBox::new(Vec3::ZERO, [8, 8, 0], [0, 0]) });
        let text = model.to_text();
        assert_eq!(BoxModel::parse(&text).unwrap(), model, "{text}");
        assert!(text.contains("parent = \"Body\""), "a parent is named, not numbered: {text}");
    }

    #[test]
    fn a_sheet_of_few_colours_is_kept_as_a_palette_and_one_of_many_as_it_is() {
        let mut few = Sheet::filled(64, 64, [0; 4]);
        few.fill([8, 8], [8, 8], [180, 120, 90, 255]);
        let text = few.to_text();
        assert!(text.len() < 200, "64 × 64 pixels of two colours in {} letters", text.len());
        assert_eq!(Sheet::from_text(64, 64, &text).unwrap(), few);

        let mut many = Sheet::filled(32, 32, [0, 0, 0, 255]);
        for at in 0..32 * 32 {
            many.set(at % 32, at / 32, [(at % 256) as u8, (at / 256) as u8, 7, 255]);
        }
        assert_eq!(Sheet::from_text(32, 32, &many.to_text()).unwrap(), many);
        assert!(Sheet::from_text(16, 16, &many.to_text()).is_err(), "pixels for another size of sheet");
    }

    #[test]
    fn a_file_with_a_parent_after_its_child_or_two_bones_of_a_name_is_refused() {
        let late = "sheet = [16, 16]\n[[bones]]\nname = \"Head\"\nparent = \"Body\"\n[[bones]]\nname = \"Body\"\n";
        assert!(BoxModel::parse(late).is_err());
        let twice = "sheet = [16, 16]\n[[bones]]\nname = \"Body\"\n[[bones]]\nname = \"Body\"\n";
        assert!(BoxModel::parse(twice).is_err());
        let both = "sheet = [16, 16]\n[[bones]]\nname = \"Body\"\n[[bones.boxes]]\nfrom = [0, 0, 0]\nsize = [1, 1, 1]\nuv = [0, 0]\nfaces = { top = [0, 0, 1, 1] }\n";
        assert!(BoxModel::parse(both).is_err());
        let plain =
            "sheet = [16, 16]\n[[bones]]\nname = \"Body\"\n[[bones.boxes]]\nfrom = [0, 0, 0]\nsize = [1, 1, 1]\n";
        let model = BoxModel::parse(plain).unwrap();
        assert_eq!(model.pixels_per_block, PIXELS_PER_BLOCK);
        assert_eq!(model.sheet.get(3, 3), [255; 4], "a file with no pixels is white");
    }

    #[test]
    fn an_unfolded_boxs_faces_go_round_it_on_the_sheet() {
        let body = ModelBox::new(Vec3::ZERO, [8, 12, 4], [16, 16]);
        assert_eq!(body.unfolded_size(), [24, 16]);
        assert_eq!(body.face_rect(Face::Top), Some([20.0, 16.0, 28.0, 20.0]));
        assert_eq!(body.face_rect(Face::Bottom), Some([28.0, 16.0, 36.0, 20.0]));
        assert_eq!(body.face_rect(Face::Right), Some([16.0, 20.0, 20.0, 32.0]));
        assert_eq!(body.face_rect(Face::Front), Some([20.0, 20.0, 28.0, 32.0]));
        assert_eq!(body.face_rect(Face::Left), Some([28.0, 20.0, 32.0, 32.0]));
        assert_eq!(body.face_rect(Face::Back), Some([32.0, 20.0, 40.0, 32.0]));

        let mirrored = ModelBox { skin: BoxSkin::Unfolded { at: [16, 16], mirror: true }, ..body };
        assert_eq!(mirrored.face_rect(Face::Left), Some([20.0, 20.0, 16.0, 32.0]), "the right's place, turned over");
        assert_eq!(mirrored.face_rect(Face::Front), Some([28.0, 20.0, 20.0, 32.0]));
    }

    #[test]
    fn every_face_looks_out_and_its_pixels_are_the_right_way_round() {
        for face in Face::ALL {
            let (across, down) = face.across_and_down();
            assert!(close(across.cross(down), face.normal()), "{face:?}");
        }
        // Round the sides, where one face's pixels end the next's begin.
        let round = [Face::Right, Face::Front, Face::Left, Face::Back, Face::Right];
        for pair in round.windows(2) {
            let end = pair[0].normal() + pair[0].across_and_down().0;
            let start = pair[1].normal() - pair[1].across_and_down().0;
            assert!(close(end, start), "{pair:?}");
        }
        let mesh = figure().mesh();
        for triangle in mesh.indices.chunks_exact(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(mesh.vertices[triangle[i] as usize].position));
            let normal = Vec3::from(mesh.vertices[triangle[0] as usize].normal);
            assert!((b - a).cross(c - a).normalize().dot(normal) > 0.99, "wound to face out");
        }
    }

    #[test]
    fn a_box_is_built_in_metres_where_its_bones_put_it_and_bound_to_its_bone() {
        let model = figure();
        let mesh = model.mesh();
        assert_eq!(mesh.vertices.len(), 4 * 6 * 4, "four boxes of six faces");
        assert_eq!(mesh.skin.len(), mesh.vertices.len());
        let of = |joint: u16| {
            let own = mesh.vertices.iter().zip(&mesh.skin).filter(move |(_, skin)| skin.joints[0] == joint);
            own.fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(least, most), (vertex, _)| {
                (least.min(vertex.position.into()), most.max(vertex.position.into()))
            })
        };
        let (least, most) = of(1);
        assert!(
            close(least, Vec3::new(-0.25, 1.5, -0.25)) && close(most, Vec3::new(0.25, 2.0, 0.25)),
            "{least} {most}"
        );
        let (least, most) = of(2);
        assert!(
            close(least, Vec3::new(0.25, 0.75, -0.125)) && close(most, Vec3::new(0.5, 1.5, 0.125)),
            "{least} {most}"
        );
        assert!(mesh.skin.iter().all(|skin| skin.weights == [u16::MAX, 0, 0, 0]));

        // The front of the head: its upper left pixel is at the model's right, +X, and the top.
        let front = model.bones[1].boxes[0].face_rect(Face::Front).unwrap();
        let corner = mesh.vertices.iter().zip(&mesh.skin).find(|(vertex, skin)| {
            skin.joints[0] == 1
                && vertex.normal == [0.0, 0.0, 1.0]
                && vertex.position[0] > 0.0
                && vertex.position[1] > 1.9
        });
        let [u, v] = corner.unwrap().0.uv;
        assert!((u * 64.0 - front[0]).abs() < 0.1 && (v * 64.0 - front[1]).abs() < 0.1, "{u} {v}");
    }

    #[test]
    fn a_turned_bone_and_box_turn_what_they_hold_and_an_inflated_box_stands_out() {
        let mut model = BoxModel::default();
        let arm = model.add_bone("Arm", None, Vec3::new(0.0, 16.0, 0.0));
        model.bones[arm].rotation = Vec3::new(0.0, 0.0, 90.0);
        model.add_box(arm, Vec3::new(0.0, -1.0, -1.0), [16, 2, 2], [0, 0]);
        let mesh = model.mesh();
        let most = mesh.vertices.iter().fold(Vec3::NEG_INFINITY, |most, vertex| most.max(vertex.position.into()));
        assert!((most.y - 2.0).abs() < 1e-4, "turned a quarter about Z, it reaches a block up from its pivot: {most}");

        model.bones[arm].rotation = Vec3::ZERO;
        model.bones[arm].boxes[0].inflate = 2.0;
        let mesh = model.mesh();
        let most = mesh.vertices.iter().fold(Vec3::NEG_INFINITY, |most, vertex| most.max(vertex.position.into()));
        assert!(close(most, Vec3::new(18.0, 19.0, 3.0) / 16.0), "{most}");
    }

    #[test]
    fn a_flat_box_draws_its_two_sides_and_a_face_with_no_place_is_left_out() {
        let mut model = BoxModel::default();
        let bone = model.add_bone("Cloth", None, Vec3::ZERO);
        model.add_box(bone, Vec3::ZERO, [8, 8, 0], [0, 0]);
        assert_eq!(model.mesh().indices.len(), 2 * 6, "front and back");
        let mut faces = [None; 6];
        faces[Face::Top.index()] = Some([0.0, 0.0, 4.0, 4.0]);
        model.bones[bone].boxes[0] =
            ModelBox { skin: BoxSkin::Faces(faces), ..ModelBox::new(Vec3::ZERO, [4, 4, 4], [0, 0]) };
        let mesh = model.mesh();
        assert_eq!(mesh.indices.len(), 6);
        assert!(mesh.vertices.iter().all(|vertex| vertex.normal == [0.0, 1.0, 0.0]));
    }

    #[test]
    fn a_model_is_prepared_as_one_skinned_part_with_its_bones_and_its_sheet() {
        let model = figure();
        let prepared = model.prepared("Mobs/Figure.boxmodel");
        assert_eq!(prepared.parts().collect::<Vec<_>>(), [("mesh0.0", Some("material0"))]);
        assert_eq!((prepared.nodes[0].name.as_str(), prepared.nodes[0].skin), ("Figure", Some(0)));
        let animation = prepared.animation().unwrap();
        assert_eq!(animation.skeleton.bones.len(), 4);
        assert_eq!(animation.skeleton.find("LeftArm"), Some(3));
        assert_eq!(animation.skins[0].joints, [0, 1, 2, 3]);
        // A bound vertex stays where it was built while its bone is at rest.
        let rest = animation.skeleton.model_matrices(&animation.skeleton.rest());
        let (vertex, skin) = (prepared.meshes[0].0.vertices[30], prepared.meshes[0].0.skin[30]);
        let joint = usize::from(skin.joints[0]);
        let posed = (rest[joint] * animation.skins[0].inverse_binds[joint]).transform_point3(vertex.position.into());
        assert!(close(posed, vertex.position.into()));
        let (texture, material) = (&prepared.textures[0], &prepared.materials[0]);
        assert_eq!((texture.width, texture.height, texture.compress), (64, 64, false));
        assert!(material.desc.pixels && material.desc.alpha_cutoff.is_some() && material.base_color == Some(0));

        let bare = BoxModel { bones: vec![BoxBone::new("Root", None, Vec3::ZERO)], ..BoxModel::default() };
        let prepared = bare.prepared("Bare.boxmodel");
        assert!(prepared.nodes.is_empty() && prepared.animation().unwrap().skeleton.bones.len() == 1);
    }

    #[test]
    fn the_starter_is_a_block_on_the_ground_with_every_face_coloured() {
        let model = BoxModel::starter();
        let mesh = model.mesh();
        let (least, most) = mesh.vertices.iter().fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(least, most), vertex| {
            (least.min(vertex.position.into()), most.max(vertex.position.into()))
        });
        assert!(close(least, Vec3::new(-0.5, 0.0, -0.5)) && close(most, Vec3::new(0.5, 1.0, 0.5)));
        for face in Face::ALL {
            let [left, top, right, bottom] = model.bones[0].boxes[0].face_rect(face).unwrap();
            assert_eq!(model.sheet.get(left as u32, top as u32)[3], 255, "{face:?}");
            assert_eq!(model.sheet.get(right as u32 - 1, bottom as u32 - 1)[3], 255, "{face:?}");
        }
        assert_eq!(BoxModel::parse(&model.to_text()).unwrap(), model);
    }
}
