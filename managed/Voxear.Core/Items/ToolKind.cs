namespace Voxear;

/// <summary>
/// A kind of tool: a pickaxe, an axe, a shovel, or whatever a game has. It is nothing but its name, the file's. A
/// block's Tool names the kind that breaks it fast, and an item's Tool the kind it is, so a game adds a kind by
/// making a data asset, with no code.
/// </summary>
[CreateAsset("Items/Tool Kind")]
public class ToolKind : Definition
{
}
