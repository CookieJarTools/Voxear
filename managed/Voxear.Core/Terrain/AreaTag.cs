namespace Voxear;

/// <summary>
/// A tag of World Areas: a town, a forest, a dungeon floor, or whatever a game has. It is nothing but its name, the
/// file's. A World Area holds tags, each with a data asset of the game's own, and a structure's placement keeps within
/// areas of some tags and out of others. A game adds a tag by making a data asset, with no code.
/// </summary>
[CreateAsset("World/Area Tag")]
public class AreaTag : Definition
{
}
