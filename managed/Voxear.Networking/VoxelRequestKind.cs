namespace Voxear;

/// <summary>What a player asked of the server's endless Voxel Volumes, as <see cref="VoxelRequests"/> sees it.</summary>
public enum VoxelRequestKind
{
	/// <summary>Voxels dug one at a time: <c>Voxels.Dig</c>.</summary>
	Dig,
	/// <summary>Voxels placed one at a time: <c>Voxels.Place</c>.</summary>
	Place,
	/// <summary>Voxels let go back to what the ground makes: <c>Voxels.Restore</c>.</summary>
	Restore,
	/// <summary>A shape carved out: <c>Voxels.Carve</c>, <c>Voxels.CarveBox</c> and the rest.</summary>
	Carve,
	/// <summary>A shape filled with a ground or paving: <c>Voxels.Fill</c>, <c>Voxels.Box</c>, <c>Voxels.Area</c> and the rest.</summary>
	Fill,
	/// <summary>A Voxel Print printed: <c>Voxels.Print</c>.</summary>
	Print,
	/// <summary>Grass cut and trees felled: <c>Voxels.Cut</c> and <c>Voxels.Slice</c>.</summary>
	Cut,
	/// <summary>What was built blown out: <c>Voxels.Blast</c>.</summary>
	Blast,
	/// <summary>A rope tied: <c>Voxels.Hang</c>.</summary>
	Hang,
	/// <summary>A rope cut: <c>Voxels.CutRope</c>.</summary>
	CutRope,
}
