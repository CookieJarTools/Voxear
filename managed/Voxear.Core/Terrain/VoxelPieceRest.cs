namespace Voxear;

/// <summary>
/// What becomes of a felled piece once it has fallen and lain still for a second. Left out of a cut, the volume's
/// Felled setting says.
/// </summary>
public enum VoxelPieceRest
{
	/// <summary>It stays a body of its own, which can be pushed and rolled.</summary>
	Stay,

	/// <summary>It becomes voxels of its volume again where it lies, solid as the ground, and its Obj goes.</summary>
	Stamp,

	/// <summary>It is taken away where it lies.</summary>
	Remove,
}
