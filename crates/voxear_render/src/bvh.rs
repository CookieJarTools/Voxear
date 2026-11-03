//! Bounding volume hierarchies, for tracing rays in a compute shader on GPUs
//! without ray tracing: one over each mesh's triangles, built when the mesh
//! loads, and one over the scene's instances, built each frame. The shader
//! that walks them is `shaders/trace_software.slang`, which `ddgi.slang`
//! includes with `SOFTWARE_RAYS`.
//!
//! Each is built by binned SAH: every split tries a few planes along the
//! longest axis and keeps the one that makes rays cheapest to trace, by the
//! surface areas either side.

use std::collections::HashMap;

use anyhow::{Context, Result};
use voxear_core::math::Vec3;
use voxear_rhi::{Buffer, Gpu, MemoryLocation, vk};

use crate::alloc::RangeAllocator;

/// A node as the shader reads it: its box, and either its first child (the
/// second follows it) or, with `count` above 0, its first primitive in the
/// hierarchy's order.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BvhNode {
    pub min: [f32; 3],
    pub left_or_first: u32,
    pub max: [f32; 3],
    pub count: u32,
}

/// A hierarchy: its nodes, the root first, and its primitives' indices in
/// the order its leaves name them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bvh {
    pub nodes: Vec<BvhNode>,
    pub order: Vec<u32>,
}

const BINS: usize = 12;

#[derive(Clone, Copy)]
struct Bounds {
    min: Vec3,
    max: Vec3,
}

impl Bounds {
    const EMPTY: Self = Self { min: Vec3::splat(f32::MAX), max: Vec3::splat(f32::MIN) };

    fn grow(self, other: Self) -> Self {
        Self { min: self.min.min(other.min), max: self.max.max(other.max) }
    }

    fn area(self) -> f32 {
        let size = (self.max - self.min).max(Vec3::ZERO);
        size.x * size.y + size.y * size.z + size.z * size.x
    }
}

/// Builds a hierarchy over primitives given by their boxes, min and max
/// corners, with at most `leaf` primitives in a leaf.
pub fn build(boxes: &[(Vec3, Vec3)], leaf: usize) -> Bvh {
    let mut order: Vec<u32> = (0..boxes.len() as u32).collect();
    let boxed = |i: u32| Bounds { min: boxes[i as usize].0, max: boxes[i as usize].1 };
    let centre = |i: u32| (boxes[i as usize].0 + boxes[i as usize].1) * 0.5;
    let mut nodes = vec![BvhNode::default()];
    // Nodes to split: the node, and its primitives' range in `order`.
    let mut pending = vec![(0usize, 0usize, boxes.len())];
    while let Some((node, start, count)) = pending.pop() {
        let bounds = order[start..start + count].iter().fold(Bounds::EMPTY, |b, &i| b.grow(boxed(i)));
        let leaf_node = BvhNode {
            min: bounds.min.to_array(),
            left_or_first: start as u32,
            max: bounds.max.to_array(),
            count: count as u32,
        };
        let Some(middle) = split(&mut order[start..start + count], bounds, &boxed, &centre, leaf) else {
            nodes[node] = leaf_node;
            continue;
        };
        let left = nodes.len();
        nodes.extend([BvhNode::default(); 2]);
        nodes[node] = BvhNode { count: 0, left_or_first: left as u32, ..leaf_node };
        pending.push((left, start, middle));
        pending.push((left + 1, start + middle, count - middle));
    }
    if boxes.is_empty() {
        nodes[0] = BvhNode::default();
    }
    Bvh { nodes, order }
}

/// Splits `items` in place along the cheapest of a few planes, returning
/// how many fall on the near side; `None` when a leaf costs less.
fn split(
    items: &mut [u32],
    bounds: Bounds,
    boxed: &impl Fn(u32) -> Bounds,
    centre: &impl Fn(u32) -> Vec3,
    leaf: usize,
) -> Option<usize> {
    if items.len() <= leaf {
        return None;
    }
    let (low, high) = items.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), &i| {
        let c = centre(i);
        (lo.min(c), hi.max(c))
    });
    let extent = high - low;
    let axis = if extent.x >= extent.y && extent.x >= extent.z {
        0
    } else if extent.y >= extent.z {
        1
    } else {
        2
    };
    if extent[axis] <= 1e-9 {
        // Every centre in one place: split down the middle of the list instead.
        return Some(items.len() / 2);
    }
    let bin_of = |i: u32| (((centre(i)[axis] - low[axis]) / extent[axis] * BINS as f32) as usize).min(BINS - 1);
    let mut bins = [(Bounds::EMPTY, 0usize); BINS];
    for &i in items.iter() {
        let bin = &mut bins[bin_of(i)];
        *bin = (bin.0.grow(boxed(i)), bin.1 + 1);
    }
    // The cost of each plane between bins: the boxes' areas either side, by how many each holds.
    let mut best = (f32::MAX, 0);
    for plane in 1..BINS {
        let side = |range: std::ops::Range<usize>| {
            bins[range].iter().fold((Bounds::EMPTY, 0), |(b, n), &(bin, count)| (b.grow(bin), n + count))
        };
        let ((near, near_count), (far, far_count)) = (side(0..plane), side(plane..BINS));
        if near_count == 0 || far_count == 0 {
            continue;
        }
        let cost = near.area() * near_count as f32 + far.area() * far_count as f32;
        if cost < best.0 {
            best = (cost, plane);
        }
    }
    let leaf_cost = bounds.area() * items.len() as f32;
    if best.0 >= leaf_cost && items.len() <= leaf * 4 {
        return None;
    }
    if best.0 == f32::MAX {
        return Some(items.len() / 2);
    }
    let middle = partition(items, |&i| bin_of(i) < best.1);
    Some(middle.clamp(1, items.len() - 1))
}

/// Puts the items for which `near` holds first, returning how many.
fn partition(items: &mut [u32], near: impl Fn(&u32) -> bool) -> usize {
    let mut first_far = 0;
    for i in 0..items.len() {
        if near(&items[i]) {
            items.swap(i, first_far);
            first_far += 1;
        }
    }
    first_far
}

/// Each triangle's box, from a mesh's points and three indices a triangle.
pub fn triangle_boxes(points: &[Vec3], indices: &[u32]) -> Vec<(Vec3, Vec3)> {
    indices
        .chunks_exact(3)
        .map(|t| {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| points.get(i as usize).copied().unwrap_or(Vec3::ZERO));
            (a.min(b).min(c), a.max(b).max(c))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every primitive appears once in a leaf, and every node's box holds its
    /// primitives' boxes.
    fn check(bvh: &Bvh, boxes: &[(Vec3, Vec3)]) {
        let mut seen = vec![0; boxes.len()];
        let mut pending = vec![0usize];
        while let Some(index) = pending.pop() {
            let node = bvh.nodes[index];
            let (min, max) = (Vec3::from(node.min), Vec3::from(node.max));
            if node.count == 0 {
                pending.extend([node.left_or_first as usize, node.left_or_first as usize + 1]);
                continue;
            }
            for &i in &bvh.order[node.left_or_first as usize..(node.left_or_first + node.count) as usize] {
                seen[i as usize] += 1;
                let (b_min, b_max) = boxes[i as usize];
                assert!(b_min.cmpge(min - 1e-5).all() && b_max.cmple(max + 1e-5).all(), "a leaf's box holds its own");
            }
        }
        assert!(seen.iter().all(|&n| n == 1), "every primitive once: {seen:?}");
    }

    #[test]
    fn a_grid_of_boxes_splits_into_small_leaves_holding_each_once() {
        let boxes: Vec<_> = (0..1000)
            .map(|i| {
                let at = Vec3::new((i % 10) as f32, (i / 10 % 10) as f32, (i / 100) as f32);
                (at, at + 0.5)
            })
            .collect();
        let bvh = build(&boxes, 4);
        check(&bvh, &boxes);
        let deepest = bvh.nodes.iter().filter(|n| n.count > 0).map(|n| n.count).max().unwrap();
        assert!(deepest <= 16, "leaves stay small: {deepest}");
        assert!(bvh.nodes.len() > 200, "it splits: {} nodes", bvh.nodes.len());
    }

    #[test]
    fn primitives_all_in_one_place_still_split_and_one_is_a_leaf() {
        let boxes = vec![(Vec3::ZERO, Vec3::ONE); 40];
        check(&build(&boxes, 4), &boxes);
        let one = vec![(Vec3::ZERO, Vec3::ONE)];
        let bvh = build(&one, 4);
        assert_eq!(bvh.nodes.len(), 1);
        assert_eq!(bvh.nodes[0].count, 1);
    }
}

/// Each mesh's hierarchy on the GPU, for tracing without ray tracing: every
/// mesh's nodes in one buffer and its triangles' order in another, and a
/// table, by mesh slot, of where each mesh's start.
pub struct MeshHierarchies {
    pub nodes: Buffer,
    pub order: Buffer,
    pub table: Buffer,
    node_space: RangeAllocator,
    order_space: RangeAllocator,
    /// Each mesh's nodes and order, where they lie.
    placed: HashMap<u32, ((u64, u64), (u64, u64))>,
}

/// How many nodes and triangles all meshes' hierarchies hold at most.
const MAX_NODES: u64 = 2 * 1024 * 1024;
const MAX_ORDER: u64 = 4 * 1024 * 1024;
/// Triangles in a mesh hierarchy's leaf at most, and instances in the scene's.
pub const MESH_LEAF: usize = 4;
pub const SCENE_LEAF: usize = 2;

impl MeshHierarchies {
    pub fn new(gpu: &Gpu, meshes: u64) -> Result<Self> {
        let usage = vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST;
        let node_size = size_of::<BvhNode>() as u64;
        Ok(Self {
            nodes: gpu.create_buffer("bvh nodes", MAX_NODES * node_size, usage, MemoryLocation::GpuOnly)?,
            order: gpu.create_buffer("bvh order", MAX_ORDER * 4, usage, MemoryLocation::GpuOnly)?,
            table: gpu.create_buffer("bvh table", meshes * 8, usage, MemoryLocation::GpuOnly)?,
            node_space: RangeAllocator::new(MAX_NODES),
            order_space: RangeAllocator::new(MAX_ORDER),
            placed: HashMap::new(),
        })
    }

    /// Builds the hierarchy of the mesh in table slot `slot` from its finest
    /// LOD's points and indices, replacing any it had.
    pub fn add(&mut self, gpu: &Gpu, slot: u32, points: &[Vec3], indices: &[u32]) -> Result<()> {
        self.remove(slot);
        let bvh = build(&triangle_boxes(points, indices), MESH_LEAF);
        let (node_count, order_count) = (bvh.nodes.len() as u64, bvh.order.len().max(1) as u64);
        let nodes = self.node_space.alloc(node_count).context("out of BVH node space")?;
        let Some(order) = self.order_space.alloc(order_count) else {
            self.node_space.free(nodes, node_count);
            anyhow::bail!("out of BVH triangle space");
        };
        gpu.upload_buffer(&self.nodes, nodes * size_of::<BvhNode>() as u64, bytemuck::cast_slice(&bvh.nodes))?;
        if !bvh.order.is_empty() {
            gpu.upload_buffer(&self.order, order * 4, bytemuck::cast_slice(&bvh.order))?;
        }
        let entry = [nodes as u32, order as u32];
        gpu.upload_buffer(&self.table, u64::from(slot) * 8, bytemuck::cast_slice(&entry))?;
        self.placed.insert(slot, ((nodes, node_count), (order, order_count)));
        Ok(())
    }

    /// Gives back a mesh's hierarchy's space, once nothing reads it.
    pub fn remove(&mut self, slot: u32) {
        if let Some(((nodes, node_count), (order, order_count))) = self.placed.remove(&slot) {
            self.node_space.free(nodes, node_count);
            self.order_space.free(order, order_count);
        }
    }

    pub fn destroy(self, gpu: &Gpu) {
        for buffer in [self.nodes, self.order, self.table] {
            gpu.destroy_buffer(buffer);
        }
    }
}

/// One instance as the scene's hierarchy names it, for the shader: the
/// inverse of its model matrix's first three rows, which takes rays into
/// the mesh's own space, and which instance and mesh it is.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TraceInstance {
    pub inverse: [[f32; 4]; 3],
    pub instance: u32,
    pub mesh: u32,
    pub _pad: [u32; 2],
}

/// The scene's hierarchy over its instances' boxes in the world, built each
/// frame, with its leaves' instances in its order.
pub fn scene(instances: &[(TraceInstance, (Vec3, Vec3))]) -> (Vec<BvhNode>, Vec<TraceInstance>) {
    let boxes: Vec<_> = instances.iter().map(|(_, bounds)| *bounds).collect();
    let bvh = build(&boxes, SCENE_LEAF);
    let ordered = bvh.order.iter().map(|&i| instances[i as usize].0).collect();
    (bvh.nodes, ordered)
}
