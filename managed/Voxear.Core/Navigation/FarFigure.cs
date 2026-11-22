using System.Numerics;

namespace Voxear;

/// <summary>
/// A figure for <see cref="FarFigures"/> to draw far off: a prefab's models standing or walking at a place, with no
/// Obj of its own. Its Id ties it to what was drawn for it before, so a figure set again further on walks there.
/// </summary>
public readonly struct FarFigure
{
	public FarFigure(uint id, Prefab prefab, Vector3 position, float yaw = 0f, float speed = 0f)
	{
		(Id, Prefab, Position, Yaw, Speed) = (id, prefab, position, yaw, speed);
	}

	/// <summary>Its own number, the same each time it is set: a resident's, say.</summary>
	public uint Id { get; }

	/// <summary>The prefab whose models it is drawn with, playing their walk or idle clips.</summary>
	public Prefab Prefab { get; }

	/// <summary>Where it stands, in the world.</summary>
	public Vector3 Position { get; }

	/// <summary>Which way it faces about up, in radians: 0 along +Z.</summary>
	public float Yaw { get; }

	/// <summary>How fast it walks, in metres a second: 0 stands it still, playing its idle clip.</summary>
	public float Speed { get; }
}
