namespace Voxear;

/// <summary>A prefab Placing makes, as often as its weight says among the others.</summary>
public readonly record struct PlacingChoice(Prefab Prefab, int Weight = 1);
