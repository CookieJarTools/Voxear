namespace Voxear;

/// <summary>
/// One control of a binding: the part it plays in a composite, such as Up, or null for a plain binding; the control,
/// or null when the binding has none yet; and its label as the player's keyboard or pad names it.
/// </summary>
public readonly record struct InputBindingControl(BindingPart? Part, InputControl? Control, string Label);
