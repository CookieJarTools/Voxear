namespace Voxear;

/// <summary>
/// What <see cref="Voxels.Fill"/>, <see cref="Voxels.Box"/> and the other shapes fill with: a ground, its voxels in
/// the shades the Voxel Volume's own ground or its generator's has, or a paving, laid in a pattern of its own with
/// joints between its stones, bricks or boards.
/// </summary>
public enum VoxelMaterial
{
	/// <summary>Dirt, as under grass.</summary>
	Dirt,

	/// <summary>Dirt with the grass's green on its top, open to the sky.</summary>
	Grass,

	Rock,
	Sand,
	Snow,
	Gravel,

	/// <summary>Cobbles a fifth of a metre across, each row set half a stone along.</summary>
	Cobbles,

	/// <summary>Flagstones 0.6 by 0.45 metres, each row set half a stone along.</summary>
	Flagstones,

	/// <summary>Bricks a quarter of a metre long and an eighth tall, each course set half a brick along.</summary>
	Bricks,

	/// <summary>Boards a fifth of a metre wide and two metres long, along X.</summary>
	Planks,

	/// <summary>Dressed stone blocks half a metre long and a quarter tall, each course set half a block along.</summary>
	Ashlar,
}
