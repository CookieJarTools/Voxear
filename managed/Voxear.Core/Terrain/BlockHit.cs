using System.Numerics;

namespace Voxear;

/// <summary>
/// Where a ray met a block in a Block World: the world's Obj, the point on the face it met, the way that face faces,
/// how far along the ray, the middle of the block and how large a block is, and which block it is: its
/// <see cref="Definition"/>, and its number in its world.
/// </summary>
public readonly struct BlockHit(Obj obj, Vector3 point, Vector3 normal, float distance, Vector3 middle, float blockSize, ushort block)
{
	public Obj Obj { get; } = obj;
	public Vector3 Point { get; } = point;
	public Vector3 Normal { get; } = normal;
	public float Distance { get; } = distance;
	public Vector3 Middle { get; } = middle;
	public float BlockSize { get; } = blockSize;
	public ushort Block { get; } = block;

	/// <summary>
	/// The block it is: its name, how hard it is, what it drops. None for a kind its world keeps itself, a colour
	/// alone, which has no rules.
	/// </summary>
	public Block Definition => BlockInfos.Numbered(Obj, Block);

	/// <summary>The middle of the block in the air before the face it met, where one placed against it goes.</summary>
	public Vector3 Before => Middle + Normal * BlockSize;
}
