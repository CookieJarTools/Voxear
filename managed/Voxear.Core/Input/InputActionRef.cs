using System;

namespace Voxear;

/// <summary>
/// One of the project's input actions, as a field holds it: picked in the Inspector from the project's actions, or
/// named in code by the Actions class made from them, <c>Actions.Player.Jump</c>. It names the action by its id, so
/// renaming the action, or moving it to another map, keeps the field pointing at it.
/// <code>
/// [SerializeField] private InputActionRef jump;
///
/// protected override void Update()
/// {
/// 	if (jump.Action.Pressed)
/// 	{
/// 		Leap();
/// 	}
/// }
/// </code>
/// </summary>
public readonly struct InputActionRef : IEquatable<InputActionRef>
{
	public static readonly InputActionRef None = default;

	private readonly string? id;

	/// <summary>The action with this id, or with this name, as "Player/Jump", when it has no id.</summary>
	public InputActionRef(string id)
	{
		this.id = id;
	}

	/// <summary>Whether it names an action.</summary>
	public bool IsSet => !string.IsNullOrEmpty(id);

	/// <summary>The action's id, as the project's input actions file gives it.</summary>
	public string Id => id ?? string.Empty;

	/// <summary>The action, found by its id. One the project no longer has reads nothing, and its Exists is false.</summary>
	public InputAction Action => Input.Action(Id);

	public static implicit operator InputAction(InputActionRef reference)
	{
		return reference.Action;
	}

	public static bool operator ==(InputActionRef left, InputActionRef right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(InputActionRef left, InputActionRef right)
	{
		return !left.Equals(right);
	}

	public bool Equals(InputActionRef other)
	{
		return Id == other.Id;
	}

	public override bool Equals(object? other)
	{
		return other is InputActionRef reference && Equals(reference);
	}

	public override int GetHashCode()
	{
		return Id.GetHashCode();
	}

	public override string ToString()
	{
		return IsSet ? Action.Name : "no input action";
	}
}
