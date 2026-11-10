//! Models of small coloured blocks from `.vox` files, as voxel sculpting
//! tools save them: each model's blocks, each a colour of the file's
//! palette of 255, placed as the file's scene places its models, z up in the
//! file and y up here.
//!
//! A `.vox` file is `VOX `, a version, then chunks, each its id, the size of
//! its contents and of its children, its contents and its children: under
//! `MAIN`, each model's `SIZE` and `XYZI` (its blocks, a byte each for x, y,
//! z and colour), the palette `RGBA`, and the scene: transforms (`nTRN`)
//! over groups (`nGRP`) and shapes (`nSHP`), which name the models.

use std::collections::HashMap;

use anyhow::{Context, Result, bail, ensure};

/// One model's blocks: each block's place, z up as the file has it, and
/// its colour's place in the palette, from 1.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoxModel {
    pub size: [u32; 3],
    pub blocks: Vec<([u8; 3], u8)>,
}

/// What a `.vox` file holds: its models, each placed where its scene puts
/// it (its middle there, z up), and its palette of 255 colours in sRGB, the
/// first for colour 1.
#[derive(Clone, Debug, PartialEq)]
pub struct VoxFile {
    pub models: Vec<(VoxModel, [i32; 3])>,
    pub palette: [[f32; 3]; 255],
}

/// A chunk: its id, its contents and its children.
struct Part<'a> {
    id: [u8; 4],
    content: &'a [u8],
    children: &'a [u8],
}

/// The chunks one after another in `bytes`.
fn parts(mut bytes: &[u8]) -> Result<Vec<Part<'_>>> {
    let mut found = Vec::new();
    while !bytes.is_empty() {
        ensure!(bytes.len() >= 12, "a chunk cut short");
        let id = [bytes[0], bytes[1], bytes[2], bytes[3]];
        let content = u32::from_le_bytes(bytes[4..8].try_into().expect("four bytes")) as usize;
        let children = u32::from_le_bytes(bytes[8..12].try_into().expect("four bytes")) as usize;
        let rest = &bytes[12..];
        ensure!(rest.len() >= content + children, "a chunk longer than its file");
        found.push(Part { id, content: &rest[..content], children: &rest[content..content + children] });
        bytes = &rest[content + children..];
    }
    Ok(found)
}

/// A reader of a chunk's contents.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, count: usize) -> Result<&[u8]> {
        let taken = self.bytes.get(self.at..self.at + count).context("a chunk's contents end early")?;
        self.at += count;
        Ok(taken)
    }

    fn int(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().expect("four bytes")))
    }

    fn text(&mut self) -> Result<String> {
        let length = self.int()?.max(0) as usize;
        Ok(String::from_utf8_lossy(self.take(length)?).into_owned())
    }

    /// A dictionary of text keys and values.
    fn dictionary(&mut self) -> Result<HashMap<String, String>> {
        let count = self.int()?.max(0);
        let mut found = HashMap::new();
        for _ in 0..count {
            let key = self.text()?;
            found.insert(key, self.text()?);
        }
        Ok(found)
    }
}

/// A node of a file's scene: a transform's translation and child, a
/// group's children, or a shape's models.
enum Node {
    Transform { translation: [i32; 3], child: i32 },
    Group(Vec<i32>),
    Shape(Vec<i32>),
}

impl VoxFile {
    pub fn read(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() >= 8 && &bytes[..4] == b"VOX ", "not a .vox file");
        let top = parts(&bytes[8..])?;
        let main = top.into_iter().find(|part| &part.id == b"MAIN").context("a .vox file with no MAIN chunk")?;
        let mut models: Vec<VoxModel> = Vec::new();
        let mut size = [0u32; 3];
        let mut palette = default_palette();
        let mut nodes: HashMap<i32, Node> = HashMap::new();
        for part in parts(main.children)? {
            let mut read = Reader { bytes: part.content, at: 0 };
            match &part.id {
                b"SIZE" => size = [read.int()?, read.int()?, read.int()?].map(|v| v.max(0) as u32),
                b"XYZI" => {
                    let count = read.int()?.max(0) as usize;
                    let mut blocks = Vec::with_capacity(count);
                    for _ in 0..count {
                        let b = read.take(4)?;
                        if b[3] != 0 {
                            blocks.push(([b[0], b[1], b[2]], b[3]));
                        }
                    }
                    models.push(VoxModel { size, blocks });
                }
                b"RGBA" => {
                    for colour in &mut palette {
                        let c = read.take(4)?;
                        *colour = [c[0], c[1], c[2]].map(|v| f32::from(v) / 255.0);
                    }
                }
                b"nTRN" => {
                    let id = read.int()?;
                    read.dictionary()?;
                    let child = read.int()?;
                    read.int()?;
                    read.int()?;
                    let frames = read.int()?.max(0);
                    let mut translation = [0; 3];
                    for frame in 0..frames {
                        let attributes = read.dictionary()?;
                        if let (0, Some(t)) = (frame, attributes.get("_t")) {
                            let values: Vec<i32> = t.split_whitespace().filter_map(|v| v.parse().ok()).collect();
                            if let [x, y, z] = values[..] {
                                translation = [x, y, z];
                            }
                        }
                    }
                    nodes.insert(id, Node::Transform { translation, child });
                }
                b"nGRP" => {
                    let id = read.int()?;
                    read.dictionary()?;
                    let count = read.int()?.max(0);
                    let children = (0..count).map(|_| read.int()).collect::<Result<Vec<_>>>()?;
                    nodes.insert(id, Node::Group(children));
                }
                b"nSHP" => {
                    let id = read.int()?;
                    read.dictionary()?;
                    let count = read.int()?.max(0);
                    let mut shapes = Vec::new();
                    for _ in 0..count {
                        shapes.push(read.int()?);
                        read.dictionary()?;
                    }
                    nodes.insert(id, Node::Shape(shapes));
                }
                _ => {}
            }
        }
        if models.is_empty() {
            bail!("a .vox file with no models");
        }
        // Each model placed by the transforms over it, from the scene's root; without a scene, each at the middle.
        let mut placed: Vec<(VoxModel, [i32; 3])> = Vec::new();
        if nodes.contains_key(&0) {
            let mut stack = vec![(0, [0i32; 3], 0usize)];
            while let Some((id, offset, depth)) = stack.pop() {
                if depth > 64 {
                    bail!("a .vox scene nested too deep");
                }
                match nodes.get(&id) {
                    Some(Node::Transform { translation, child }) => {
                        let moved = [0, 1, 2].map(|i| offset[i] + translation[i]);
                        stack.push((*child, moved, depth + 1));
                    }
                    Some(Node::Group(children)) => {
                        // Last first, so the first child comes off first and models keep their order.
                        stack.extend(children.iter().rev().map(|&child| (child, offset, depth + 1)));
                    }
                    Some(Node::Shape(shapes)) => {
                        for &model in shapes {
                            if let Some(found) = models.get(model.max(0) as usize) {
                                placed.push((found.clone(), offset));
                            }
                        }
                    }
                    None => {}
                }
            }
        }
        if placed.is_empty() {
            placed = models.into_iter().map(|model| (model, [0; 3])).collect();
        }
        Ok(Self { models: placed, palette })
    }

    /// Every block of every model, each where it is, y up, the lowest at 0
    /// and the rest from the scene's middle, with its colour's place in the
    /// palette.
    pub fn blocks(&self) -> Vec<([i32; 3], u8)> {
        let mut all = Vec::new();
        for (model, at) in &self.models {
            let half = model.size.map(|v| (v / 2) as i32);
            for &([x, y, z], colour) in &model.blocks {
                let p =
                    [at[0] + i32::from(x) - half[0], at[1] + i32::from(y) - half[1], at[2] + i32::from(z) - half[2]];
                // z up in the file, y up here; the file's y runs along -z here, so a model is not mirrored.
                all.push(([p[0], p[2], -p[1]], colour));
            }
        }
        let lowest = all.iter().map(|(p, _)| p[1]).min().unwrap_or(0);
        for (p, _) in &mut all {
            p[1] -= lowest;
        }
        all
    }
}

/// `vox`'s blocks as changes to a world whose blocks `file` lists, standing
/// at `at` (its lowest blocks there, its middle over it): each of its
/// colours a kind of block of that colour, those `file` has already as kinds
/// of its own with no material kept, the rest added to its list while it
/// has `room` for more ways of drawing a face, and the colours past that
/// the nearest such kind there is. With no kind of a colour at all, those
/// blocks are left out.
pub fn place(
    file: &mut super::file::BlockFile,
    vox: &VoxFile,
    at: [i32; 3],
    mut room: usize,
) -> Vec<([i32; 3], super::chunk::BlockId)> {
    use super::file::{BlockKind, MAX_BLOCKS};
    let blocks = vox.blocks();
    // A colour builds only with a kind of the world's own that is a colour alone: never a block, nor water.
    let colour_alone = |kind: &BlockKind| {
        kind.is_inline() && !kind.water && kind.material.is_none() && kind.top.is_none() && kind.bottom.is_none()
    };
    let mut kind_of: HashMap<u8, Option<super::chunk::BlockId>> = HashMap::new();
    for &(_, colour) in &blocks {
        if kind_of.contains_key(&colour) {
            continue;
        }
        let wanted = vox.palette[usize::from(colour.max(1)) - 1];
        let same = file.blocks.iter().position(|kind| colour_alone(kind) && kind.color == wanted);
        let kind = match same {
            Some(found) => Some(found),
            None if room > 0 && file.blocks.len() < MAX_BLOCKS => {
                room -= 1;
                file.blocks.push(BlockKind::colored(&format!("Color {colour}"), wanted));
                Some(file.blocks.len() - 1)
            }
            // No room for more kinds: the nearest colour there is.
            None => {
                let away = |c: [f32; 3]| (0..3).map(|i| (c[i] - wanted[i]).powi(2)).sum::<f32>();
                (0..file.blocks.len())
                    .filter(|&kind| colour_alone(&file.blocks[kind]))
                    .min_by(|&a, &b| away(file.blocks[a].color).total_cmp(&away(file.blocks[b].color)))
            }
        };
        kind_of.insert(colour, kind.map(|kind| (kind + 1) as super::chunk::BlockId));
    }
    blocks
        .into_iter()
        .filter_map(|(p, colour)| Some(([at[0] + p[0], at[1] + p[1], at[2] + p[2]], kind_of[&colour]?)))
        .collect()
}

/// A palette for files with none of their own: greys and the colour wheel,
/// light to dark.
fn default_palette() -> [[f32; 3]; 255] {
    std::array::from_fn(|i| {
        let i = i as f32;
        if i < 31.0 {
            [i / 30.0; 3]
        } else {
            let hue = (i - 31.0) / 224.0 * 8.0;
            let light = 0.35 + 0.6 * (1.0 - (i - 31.0) % 28.0 / 28.0);
            [0.0, 2.0, 4.0].map(|offset: f32| {
                (light * (0.6 + 0.4 * ((hue + offset / 3.0) * std::f32::consts::TAU).cos())).clamp(0.0, 1.0)
            })
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A chunk as bytes.
    fn chunk(id: &[u8; 4], content: &[u8], children: &[u8]) -> Vec<u8> {
        let mut bytes = id.to_vec();
        bytes.extend((content.len() as u32).to_le_bytes());
        bytes.extend((children.len() as u32).to_le_bytes());
        bytes.extend(content);
        bytes.extend(children);
        bytes
    }

    fn ints(values: &[i32]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    #[test]
    fn a_vox_files_models_come_y_up_placed_by_its_scene_in_its_palettes_colours() {
        // Two models, 2 by 3 by 4 and 1 by 1 by 1, the second moved 10 along x by its transform.
        let mut first = ints(&[3]);
        for b in [[0u8, 0, 0, 1], [1, 2, 3, 2], [1, 0, 0, 0]] {
            first.extend(b);
        }
        let mut second = ints(&[1]);
        second.extend([0u8, 0, 0, 5]);
        let mut palette = Vec::new();
        for i in 0..256u32 {
            // Colour n has a red of n, for n from 1.
            palette.extend([(i + 1) as u8, 10, 20, 255]);
        }
        let dictionary = |pairs: &[(&str, &str)]| {
            let mut b = ints(&[pairs.len() as i32]);
            for (k, v) in pairs {
                b.extend(ints(&[k.len() as i32]));
                b.extend(k.as_bytes());
                b.extend(ints(&[v.len() as i32]));
                b.extend(v.as_bytes());
            }
            b
        };
        let transform = |id: i32, child: i32, t: Option<&str>| {
            let mut b = ints(&[id]);
            b.extend(dictionary(&[]));
            b.extend(ints(&[child, -1, 0, 1]));
            b.extend(dictionary(&t.map(|t| vec![("_t", t)]).unwrap_or_default()));
            b
        };
        let mut group = ints(&[1]);
        group.extend(dictionary(&[]));
        group.extend(ints(&[2, 2, 4]));
        let shape = |id: i32, model: i32| {
            let mut b = ints(&[id]);
            b.extend(dictionary(&[]));
            b.extend(ints(&[1, model]));
            b.extend(dictionary(&[]));
            b
        };
        let mut children = Vec::new();
        children.extend(chunk(b"SIZE", &ints(&[2, 3, 4]), &[]));
        children.extend(chunk(b"XYZI", &first, &[]));
        children.extend(chunk(b"SIZE", &ints(&[1, 1, 1]), &[]));
        children.extend(chunk(b"XYZI", &second, &[]));
        children.extend(chunk(b"nTRN", &transform(0, 1, None), &[]));
        children.extend(chunk(b"nGRP", &group, &[]));
        children.extend(chunk(b"nTRN", &transform(2, 3, Some("0 0 0")), &[]));
        children.extend(chunk(b"nSHP", &shape(3, 0), &[]));
        children.extend(chunk(b"nTRN", &transform(4, 5, Some("10 0 0")), &[]));
        children.extend(chunk(b"nSHP", &shape(5, 1), &[]));
        children.extend(chunk(b"RGBA", &palette, &[]));
        let mut bytes = b"VOX ".to_vec();
        bytes.extend(ints(&[200]));
        bytes.extend(chunk(b"MAIN", &[], &children));
        let file = VoxFile::read(&bytes).unwrap();
        assert_eq!(file.models.len(), 2);
        assert_eq!(file.models[0].0.blocks.len(), 2, "colour 0 is no block");
        let blocks = file.blocks();
        // The first model's (1, 2, 3) block: x from its middle, its z up as y, its y along -z.
        assert!(blocks.contains(&([1 - 1, 3, -(2 - 1)], 2)), "{blocks:?}");
        assert!(
            blocks.contains(&([10, 2, 0], 5)),
            "the second where its transform put it, as high as the first is low"
        );
        assert_eq!(file.palette[0], [1.0 / 255.0, 10.0 / 255.0, 20.0 / 255.0], "colour 1 is the palette's first");
        assert!(VoxFile::read(b"not a vox file").is_err());
        // Placed in a world: a kind of block each colour, those it has already kept.
        let mut world = crate::blocks::file::BlockFile::default();
        world.blocks.push(crate::blocks::file::BlockKind::colored("Red", file.palette[4]));
        let changes = place(&mut world, &file, [100, 7, 50], 250);
        assert_eq!(changes.len(), 3);
        assert_eq!(world.blocks.len(), 4 + 2, "colours 1 and 2 added, colour 5 the red it had");
        assert!(changes.contains(&([110, 9, 50], 4)), "the red kind, number 4");
        assert!(changes.iter().all(|(p, _)| p[1] >= 7), "standing at the place given");

        // A block of the same colour is no kind of the world's own, nor is a place one was taken out of: a colour
        // passes over both, and takes a number after them.
        use crate::blocks::file::{BlockFile, BlockKind};
        let red = BlockKind { color: file.palette[4], ..BlockKind::of("Blocks/Red.block") };
        let mut world = BlockFile { blocks: vec![red, BlockKind::gap()], ..BlockFile::default() };
        let changes = place(&mut world, &file, [0, 0, 0], 250);
        assert_eq!(world.blocks.len(), 2 + 3, "a kind for each of its three colours");
        assert!(changes.iter().all(|&(_, block)| block >= 3), "{changes:?}");
        // With room for one more way of drawing a face, the first colour is added and the rest are the nearest to
        // them; with no kind of a colour at all, its blocks are left out.
        let mut world = BlockFile { blocks: vec![BlockKind::of("Blocks/Red.block")], ..BlockFile::default() };
        assert_eq!(place(&mut world, &file, [0, 0, 0], 1).len(), 3);
        assert_eq!(world.blocks.len(), 2);
        let mut world = BlockFile { blocks: vec![BlockKind::of("Blocks/Red.block")], ..BlockFile::default() };
        assert!(place(&mut world, &file, [0, 0, 0], 0).is_empty());
        assert_eq!(world.blocks.len(), 1);
    }
}
