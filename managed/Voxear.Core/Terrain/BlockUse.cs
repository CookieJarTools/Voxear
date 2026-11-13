using System.Numerics;

namespace Voxear;

/// <summary>
/// A block being used by <see cref="Blocks.Use(Vector3, Obj)"/>, as <see cref="Blocks.Using"/> hears it before and
/// <see cref="Blocks.Used"/> after: where it is, which block, who used it, and after, what it did and whether it is
/// open (or on) now. A listener to Using may set Cancel to leave it as it is, as a locked door stays shut.
/// </summary>
public sealed class BlockUse(Vector3 position, Block block, Obj user)
{
	public Vector3 Position { get; } = position;

	public Block Block { get; } = block;

	/// <summary>Who used it; None for a script on its own.</summary>
	public Obj User { get; } = user;

	public BlockUseKind Kind { get; internal set; }

	/// <summary>Whether it is open, or on, now.</summary>
	public bool Now { get; internal set; }

	public bool Cancel { get; set; }
}
