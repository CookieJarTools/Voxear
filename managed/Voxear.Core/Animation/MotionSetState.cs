using System;

namespace Voxear;

/// <summary>
/// A state of a Motion Set, by its name: what a Motion Matcher's SetState plays, as a custom state for sitting.
/// Pick it in the Inspector from the project's sets.
/// </summary>
public readonly struct MotionSetState : IEquatable<MotionSetState>, IAsset<MotionSetState>
{
	public static readonly MotionSetState None = default;

	/// <summary>The set's GUID, or away from the engine its path.</summary>
	private readonly string? set;

	private readonly string? name;

	internal MotionSetState(string set, string name)
	{
		this.set = set;
		this.name = name;
	}

	/// <summary>The set it is in.</summary>
	public MotionSet Set => MotionSet.FromKey(set ?? string.Empty);

	/// <summary>Its name in the set.</summary>
	public string Name => name ?? string.Empty;

	public bool IsValid => !string.IsNullOrEmpty(name);

	string? IAsset<MotionSetState>.AssetGuid => IsValid ? $"{set}#{MotionParts.State}{name}" : null;

	/// <summary>
	/// A state by its reference, the set's path or GUID then the state, for example "Sets/Hero.motionset#state:Sitting". Logs an error and returns None when it names none.
	/// </summary>
	public static MotionSetState Load(string reference)
	{
		return MotionParts.Split(reference, MotionParts.State, out var set, out var name) ? new MotionSetState(set, name) : None;
	}

	public static bool operator ==(MotionSetState left, MotionSetState right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(MotionSetState left, MotionSetState right)
	{
		return !left.Equals(right);
	}

	public bool Equals(MotionSetState other)
	{
		return Name == other.Name && (set ?? string.Empty) == (other.set ?? string.Empty);
	}

	public override bool Equals(object? other)
	{
		return other is MotionSetState state && Equals(state);
	}

	public override int GetHashCode()
	{
		return HashCode.Combine(set ?? string.Empty, Name);
	}

	public override string ToString()
	{
		return IsValid ? Name : "MotionSetState(none)";
	}
}
