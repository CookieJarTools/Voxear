namespace Voxear;

/// <summary>
/// What a ray through an endless Voxel Volume meets (<see cref="Voxels.Raycast(System.Numerics.Vector3, System.Numerics.Vector3, out VoxelHit, float, VoxelRayMeets)"/>).
/// </summary>
public enum VoxelRayMeets
{
	/// <summary>What is solid, and the trees' leaves: the ground, logs, edits and voxels placed. As it comes.</summary>
	Solid,

	/// <summary>That, and the grass, cover and crops, as tall as they grow less what a cut left of them: what a blade
	/// swung low meets.</summary>
	Cover,
}
