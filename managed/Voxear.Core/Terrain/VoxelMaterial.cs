namespace Voxear;

/// <summary>
/// The ground <see cref="Voxels.Fill"/> fills a ball with, its voxels in the shades the Voxel Volume's own ground or
/// its generator's has.
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
}
