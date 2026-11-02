namespace Voxear;

/// <summary>A group of input actions, as key maps colour them, from the input actions' [[category]] tables.</summary>
public readonly record struct InputCategory(string Name, Color Color);
