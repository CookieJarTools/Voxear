using System.Numerics;

namespace Voxear;

/// <summary>
/// One dimension's blocks: the same calls as <see cref="Blocks"/>, asked of that dimension's worlds of blocks
/// whichever the game is seen from. Got from <see cref="DimensionInstance.Blocks"/>.
/// </summary>
public readonly struct DimensionBlocks
{
	private readonly DimensionInstance dimension;

	internal DimensionBlocks(DimensionInstance dimension)
	{
		this.dimension = dimension;
	}

	/// <summary>The number of the block at a place, 0 for air, as <see cref="Blocks.At"/>.</summary>
	public ushort At(Vector3 position)
	{
		using var scope = dimension.Use();
		return Blocks.At(position);
	}

	/// <summary>The block at a place, as <see cref="Blocks.DefinitionAt"/>.</summary>
	public Block DefinitionAt(Vector3 position)
	{
		using var scope = dimension.Use();
		return Blocks.DefinitionAt(position);
	}

	/// <summary>The top of the floor under the generated ground at a place, as <see cref="Blocks.FloorHeight"/>.</summary>
	public float FloorHeight(Vector3 position)
	{
		using var scope = dimension.Use();
		return Blocks.FloorHeight(position);
	}

	/// <summary>Puts a block, by its number in the world, at a place, as <see cref="Blocks.Set(Vector3, ushort)"/>.</summary>
	public bool Set(Vector3 position, ushort block)
	{
		using var scope = dimension.Use();
		return Blocks.Set(position, block);
	}

	/// <summary>Puts a block at a place, as <see cref="Blocks.Set(Vector3, Block)"/>.</summary>
	public bool Set(Vector3 position, Block block)
	{
		using var scope = dimension.Use();
		return Blocks.Set(position, block);
	}

	/// <summary>Fills a box with a block, as <see cref="Blocks.Fill(Vector3, Vector3, Block)"/>.</summary>
	public int Fill(Vector3 from, Vector3 to, Block block)
	{
		using var scope = dimension.Use();
		return Blocks.Fill(from, to, block);
	}

	/// <summary>Fills a ball with a block, as <see cref="Blocks.Ball(Vector3, float, Block)"/>.</summary>
	public int Ball(Vector3 middle, float radius, Block block)
	{
		using var scope = dimension.Use();
		return Blocks.Ball(middle, radius, block);
	}

	/// <summary>Breaks the blocks in a ball, as <see cref="Blocks.Break(Vector3, float)"/>.</summary>
	public int Break(Vector3 middle, float radius)
	{
		using var scope = dimension.Use();
		return Blocks.Break(middle, radius);
	}

	/// <summary>The first block a ray meets, as <see cref="Blocks.Raycast"/>.</summary>
	public bool Raycast(Vector3 origin, Vector3 direction, out BlockHit hit, float maxDistance = 1000f, bool liquids = false)
	{
		using var scope = dimension.Use();
		return Blocks.Raycast(origin, direction, out hit, maxDistance, liquids);
	}
}
