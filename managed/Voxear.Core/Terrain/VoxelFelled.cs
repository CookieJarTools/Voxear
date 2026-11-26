namespace Voxear;

/// <summary>
/// What an endless Voxel Volume's felled trees become once they have fallen and come to rest, when a cut doesn't say
/// (<see cref="Voxels.Felled"/>, <see cref="VoxelPieceRest"/>).
/// </summary>
public enum VoxelFelled
{
	/// <summary>Each stays a body of its own, which can be pushed and rolled. As it comes.</summary>
	Body,

	/// <summary>Each becomes voxels of the volume again where it lies, solid as the ground.</summary>
	Stamped,
}
