namespace Voxear;

/// <summary>A block of a Road's or an Area Fill's mix, laid as often as its weight says.</summary>
public readonly record struct MixBlock(Block Block, int Weight);
