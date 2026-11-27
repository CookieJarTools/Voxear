using System.Numerics;

namespace Voxear;

/// <summary>
/// A player's request of the server's endless Voxel Volumes, as <see cref="VoxelRequests.Check"/> sees it: who asked,
/// what kind it is, where it is in the world, how large (a ball's radius or a box's largest side, in metres), how many
/// points it sets, and the ground it fills or places, if any.
/// </summary>
public readonly record struct VoxelRequest(
	NetPlayer Player,
	VoxelRequestKind Kind,
	Vector3 At,
	float Size,
	int Points,
	VoxelMaterial? Material);
