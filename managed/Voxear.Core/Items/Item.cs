namespace Voxear;

/// <summary>
/// Something a player carries: a data asset with its icon, how many stack in one place, the block it puts down and
/// what kind of tool it is. A game derives its own from it to add fields, as a food its hunger.
/// <code>
/// // Dig what the player looks at with what they hold, and put down what it places.
/// if (Blocks.Raycast(eye, forward, out var hit, 6f))
/// {
///     if (Input.Held(dig) &amp;&amp; miner.Dig(hit, held, out var drops)) inventory.Add(drops);
///     if (Input.Pressed(use) &amp;&amp; held != null) Blocks.Place(hit, held);
/// }
/// </code>
/// </summary>
[CreateAsset("Items/Item")]
public class Item : Definition
{
	[Tooltip("Its picture in an inventory")]
	public Texture Icon;

	[LabelText("Stack")]
	[Tooltip("How many of it go in one place of an inventory")]
	[MinValue(1)]
	public int StackSize = 64;

	[Tooltip("The block it puts down")]
	public Block Places;

	[Tooltip("The item left in the hand once it is put down, as a bucket is by a bucket of water")]
	public Item? Leaves;

	[Title("Tool")]
	[Tooltip("The kind of tool it is: it breaks the blocks that ask for that kind faster")]
	public ToolKind? Tool;

	[Tooltip("How good a tool it is: a block drops what it does only to a tool of its tier or higher")]
	[MinValue(0)]
	public int Tier;

	[Tooltip("How many times faster than a hand it breaks the blocks that ask for its kind")]
	[MinValue(0.01f)]
	public float Speed = 1f;

	[Tooltip("How many blocks it breaks before it is worn out; 0 never wears")]
	[MinValue(0)]
	public int Durability;

	/// <summary>Whether it is a tool of some kind.</summary>
	public bool IsTool => Tool != null;

	/// <summary>
	/// How many times faster than a hand it breaks <paramref name="block"/>: its Speed where it is the kind of tool
	/// the block asks for, and 1 for any other block. Override it for a tool with rules of its own.
	/// </summary>
	public virtual float SpeedOn(Block block)
	{
		return Tool != null && Tool == block.Tool ? Speed : 1f;
	}

	/// <summary>
	/// Whether breaking <paramref name="block"/> with it gets the block's drops: always for a block with no tier,
	/// else only as the kind of tool the block asks for, of the block's tier or higher.
	/// </summary>
	public virtual bool Harvests(Block block)
	{
		return block.Tier <= 0 || (Tool != null && Tool == block.Tool && Tier >= block.Tier);
	}
}
