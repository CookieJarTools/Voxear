namespace Voxear;

/// <summary>
/// One control of a binding: the part it plays in a composite, such as "up", or null for a plain binding; its path,
/// such as "Keyboard/W"; and its label as the player's keyboard or pad names it.
/// </summary>
public readonly record struct InputBindingControl(string? Part, string Path, string Label);
