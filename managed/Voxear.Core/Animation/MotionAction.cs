using System;

namespace Voxear;

/// <summary>
/// An action of a Motion Set: a clip played once over the character's state, on the whole body or a part of it, as
/// swinging a wand, jumping or dying. Play one with its Motion Matcher's Play; pick it in the Inspector from the
/// project's sets.
/// <code>
/// [SerializeField] private MotionAction cast;
///
/// protected override void Update()
/// {
/// 	if (Mouse.WasPressed(MouseButton.Left))
/// 	{
/// 		Obj.MotionMatcher.Play(cast);
/// 	}
/// }
/// </code>
/// </summary>
public readonly struct MotionAction : IEquatable<MotionAction>, IAsset<MotionAction>
{
	public static readonly MotionAction None = default;

	/// <summary>The set's GUID, or away from the engine its path.</summary>
	private readonly string? set;

	private readonly string? name;

	internal MotionAction(string set, string name)
	{
		this.set = set;
		this.name = name;
	}

	/// <summary>The set it is in.</summary>
	public MotionSet Set => MotionSet.FromKey(set ?? string.Empty);

	/// <summary>Its name in the set.</summary>
	public string Name => name ?? string.Empty;

	public bool IsValid => !string.IsNullOrEmpty(name);

	string? IAsset<MotionAction>.AssetGuid => IsValid ? $"{set}#{MotionParts.Action}{name}" : null;

	/// <summary>
	/// An action by its reference, the set's path or GUID then the action, for example "Sets/Hero.motionset#action:Cast". Logs an error and returns None when it names none.
	/// </summary>
	public static MotionAction Load(string reference)
	{
		return MotionParts.Split(reference, MotionParts.Action, out var set, out var name) ? new MotionAction(set, name) : None;
	}

	public static bool operator ==(MotionAction left, MotionAction right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(MotionAction left, MotionAction right)
	{
		return !left.Equals(right);
	}

	public bool Equals(MotionAction other)
	{
		return Name == other.Name && (set ?? string.Empty) == (other.set ?? string.Empty);
	}

	public override bool Equals(object? other)
	{
		return other is MotionAction action && Equals(action);
	}

	public override int GetHashCode()
	{
		return HashCode.Combine(set ?? string.Empty, Name);
	}

	public override string ToString()
	{
		return IsValid ? Name : "MotionAction(none)";
	}
}
