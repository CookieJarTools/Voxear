namespace Voxear;

/// <summary>
/// The shape a block is drawn in: a whole cube, leaves and vines, or a block of parts, a few boxes, that joins the
/// blocks beside it as a fence or a pane does, or opens as a door does.
/// </summary>
public enum BlockShape
{
	Cube,
	Bushy,
	Vine,
	Slab,
	Stairs,
	Fence,
	Wall,
	Pane,
	Door,
	TrapDoor,
	FenceGate,
	Lever,
	Button,
	Chain,
	Rope,
	Boxes,
	/// <summary>A log: a whole block, or a post as thin as its state's Size, thinner a slice each time it is mined.</summary>
	Log,
	/// <summary>A sapling: crossed sheets standing on the block under it, which grows into the plant its block names.</summary>
	Sapling,
}
