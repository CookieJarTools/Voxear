using System.Numerics;

namespace Voxear;

/// <summary>
/// A building of a town a world of blocks' generator lays out: its middle (a well, a hall) or one of its houses. Its
/// door is on its model's -z side, turned with it, and opens onto the street.
/// </summary>
public readonly struct TownBuilding
{
	internal TownBuilding(Structure structure, bool isMiddle, Vector3 position, Vector3 size, Quaternion rotation, Vector3 front, Vector3? door)
	{
		(Structure, IsMiddle, Position, Size, Rotation, Front, Door) = (structure, isMiddle, position, size, rotation, front, door);
	}

	/// <summary>The structure it is built from: the town's middle, or one of its kinds of house.</summary>
	public Structure Structure { get; }

	/// <summary>Whether it is the town's middle rather than a house.</summary>
	public bool IsMiddle { get; }

	/// <summary>The middle of its floor: its lowest blocks' bottom, under its middle.</summary>
	public Vector3 Position { get; }

	/// <summary>How wide, tall and deep it is in metres, before it is turned.</summary>
	public Vector3 Size { get; }

	/// <summary>How it is turned about up from its model as made.</summary>
	public Quaternion Rotation { get; }

	/// <summary>Which way its door faces, along the ground.</summary>
	public Vector3 Front { get; }

	/// <summary>The middle of the block just outside its door, on its floor's level; null for a town's middle.</summary>
	public Vector3? Door { get; }

	/// <summary>A place inside it, given in its model's own metres from its floor's middle, in the world.</summary>
	public Vector3 PointOf(Vector3 local)
	{
		return Position + Vector3.Transform(local, Rotation);
	}
}
