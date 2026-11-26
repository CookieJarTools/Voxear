namespace Voxear;

/// <summary>
/// How an endless Voxel Volume's rivers and lakes are drawn (<see cref="Voxels.WaterLook"/>).
/// </summary>
public enum VoxelWaterLook
{
	/// <summary>A smooth surface over the voxels, rippling and flowing; waterfalls curve down from their lips.</summary>
	Smooth,

	/// <summary>Voxels of water as small as the ground's, shaded by how deep they are; waterfalls are voxels of water
	/// thrown over their lips, whiter the faster they fall.</summary>
	Voxels,
}
