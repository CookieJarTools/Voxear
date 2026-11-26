using System.Numerics;

namespace Voxear;

/// <summary>
/// Where a ray met an endless Voxel Volume's ground (<see cref="Voxels.Raycast(Vector3, Vector3, out VoxelHit, float, VoxelRayMeets)"/>):
/// the point, on the face of the voxel it met; which way that face looks; how far along the ray, in metres; the
/// volume's Obj; and whether what it met is grass, cover or crops, which only a ray meeting
/// <see cref="VoxelRayMeets.Cover"/> meets.
/// </summary>
public readonly record struct VoxelHit(Vector3 Point, Vector3 Normal, float Distance, Obj Obj, bool Cover = false);
