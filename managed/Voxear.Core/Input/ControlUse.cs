namespace Voxear;

/// <summary>
/// One use of a control: the action a binding of it reads, which of the action's bindings, and the part of a
/// composite one, such as "up" of a WASD binding. Label is what a key map says for it, as "Move (Up)".
/// </summary>
public readonly record struct ControlUse(InputAction Action, int Binding, string? Part, string Label, string? Category, bool Hidden);
