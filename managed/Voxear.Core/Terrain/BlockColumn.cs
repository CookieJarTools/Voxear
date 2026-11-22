using System.Numerics;

namespace Voxear;

/// <summary>
/// A column of a world of blocks without end: 32 blocks a side, from the bottom of the world to its top, made round
/// where the game is seen from and let go behind. Told of by <see cref="Blocks.ColumnMade"/> and
/// <see cref="Blocks.ColumnLetGo"/>.
/// </summary>
public readonly struct BlockColumn
{
	internal BlockColumn(Obj world, int x, int z, float size, Vector2 corner)
	{
		(World, X, Z, Size, Corner) = (world, x, z, size, corner);
	}

	/// <summary>The world of blocks it is a column of.</summary>
	public Obj World { get; }

	/// <summary>Its place among the world's columns along x, from the world's corner.</summary>
	public int X { get; }

	/// <summary>Its place among the world's columns along z, from the world's corner.</summary>
	public int Z { get; }

	/// <summary>How wide it is, in metres.</summary>
	public float Size { get; }

	/// <summary>Its low corner along x and z, in the world as it was when it was told of.</summary>
	public Vector2 Corner { get; }

	/// <summary>Its middle along x and z.</summary>
	public Vector2 Middle => Corner + new Vector2(Size * 0.5f);

	/// <summary>Whether a place stands in it, at any height.</summary>
	public bool Contains(Vector3 position)
	{
		var x = position.X - Corner.X;
		var z = position.Z - Corner.Y;
		return x >= 0f && x < Size && z >= 0f && z < Size;
	}

	public override string ToString()
	{
		return $"column ({X}, {Z}) of {World}";
	}
}
