using System.Numerics;

namespace Voxear;

/// <summary>
/// Where a ray met an endless Voxel Volume's ground (<see cref="Voxels.Raycast"/>): the point, on the face of the voxel
/// it met; which way that face looks; how far along the ray, in metres; and the volume's Obj.
/// </summary>
public readonly record struct VoxelHit(Vector3 Point, Vector3 Normal, float Distance, Obj Obj);
