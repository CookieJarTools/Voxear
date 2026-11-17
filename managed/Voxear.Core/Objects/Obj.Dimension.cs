using System.Numerics;

namespace Voxear;

public readonly partial struct Obj
{
	/// <summary>The dimension the Obj is in: its root's. The level's, for an Obj never put in another.</summary>
	public DimensionInstance Dimension => new(Interop.NativeDimensions.Of(this));

	/// <summary>
	/// Moves the Obj, with everything under it and keeping its speed, into a dimension made, at
	/// <paramref name="position"/> in that dimension's coordinates. Await it to carry on once the ground round
	/// it is made there.
	/// <code>
	/// await player.MoveTo(DimensionInstance.Level, townSquare);
	/// </code>
	/// </summary>
	public DimensionMove MoveTo(DimensionInstance dimension, Vector3 position)
	{
		return new DimensionMove(this, dimension, position);
	}

	/// <summary>
	/// Moves the Obj into the dimension <paramref name="dimension"/> makes as <paramref name="key"/>, made first
	/// where it is not, at <paramref name="position"/> in its coordinates. Await it to carry on once the Obj stands
	/// there.
	/// <code>
	/// public Ref&lt;Dimension&gt; Caves;
	/// ...
	/// await player.MoveTo(Caves.Load(), new Vector3(0f, 12f, 0f));
	/// </code>
	/// </summary>
	public DimensionMove MoveTo(Dimension dimension, Vector3 position, ulong key = 0)
	{
		return dimension.Move(this, position, key);
	}
}
