namespace Voxear;

/// <summary>
/// One use of a control: the action a binding of it reads, which of the action's bindings, and the part of a
/// composite one, such as Up of a WASD binding. Label is what a key map says for it, as "Move (Up)"; Icon the action's
/// icon, by name, or null; and Conflict whether another action that can be on at once reads the same control.
/// </summary>
public readonly record struct ControlUse(
	InputAction Action,
	int Binding,
	BindingPart? Part,
	string Label,
	string? Category,
	bool Hidden,
	string? Icon,
	bool Conflict);
