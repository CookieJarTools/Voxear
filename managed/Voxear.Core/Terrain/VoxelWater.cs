using System.Numerics;

namespace Voxear;

/// <summary>
/// The water of an endless Voxel Volume's rivers and lakes at a place (<see cref="Voxels.WaterAt"/>): how high its
/// surface stands in the world; which way and how fast its current runs, in metres a second, down the water's slope,
/// nothing on a lake; and the volume's Obj.
/// </summary>
public readonly record struct VoxelWater(float Height, Vector3 Current, Obj Obj);
