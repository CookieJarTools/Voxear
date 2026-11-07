using System;

namespace Voxear;

/// <summary>
/// A Motion Set: a .motionset file of what a character can do, shared by every character that plays it. Its states
/// are the ways it gets about, as walking or crouching, each with the looping clips a Motion Matcher blends. Its
/// actions are clips played once over them, as swinging a wand, jumping or dying. Give one to a Motion Matcher with
/// its MotionSet, or in the Inspector.
/// </summary>
public readonly struct MotionSet : IEquatable<MotionSet>, IAsset<MotionSet>
{
	public static readonly MotionSet None = default;

	/// <summary>The set's GUID, or away from the engine its path.</summary>
	private readonly string? key;

	private MotionSet(string key)
	{
		this.key = key;
	}

	public bool IsValid => !string.IsNullOrEmpty(key);

	internal string Key => key ?? string.Empty;

	string? IAsset<MotionSet>.AssetGuid => IsValid ? key : null;

	/// <summary>
	/// A Motion Set by project path or GUID, for example "Sets/Hero.motionset". Logs an error and returns None when
	/// the project has no such set.
	/// </summary>
	public static MotionSet Load(string path)
	{
		if (string.IsNullOrEmpty(path))
		{
			return None;
		}

		var found = MotionParts.SetKey(path);
		if (found == null)
		{
			Log.Error($"The project has no Motion Set \"{path}\".");
			return None;
		}
		return new MotionSet(found);
	}

	internal static MotionSet FromKey(string key)
	{
		return string.IsNullOrEmpty(key) ? None : new MotionSet(key);
	}

	public static bool operator ==(MotionSet left, MotionSet right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(MotionSet left, MotionSet right)
	{
		return !left.Equals(right);
	}

	public bool Equals(MotionSet other)
	{
		return Key == other.Key;
	}

	public override bool Equals(object? other)
	{
		return other is MotionSet set && Equals(set);
	}

	public override int GetHashCode()
	{
		return Key.GetHashCode();
	}

	public override string ToString()
	{
		return IsValid ? $"MotionSet({Key})" : "MotionSet(none)";
	}
}
