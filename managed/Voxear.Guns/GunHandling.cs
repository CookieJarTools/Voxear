using System;

namespace Voxear;

/// <summary>
/// A Gun Handling: a .gunhandling file of how a gun is held and how its actions go, as the Gun Editor sets them,
/// shared by every gun that picks it. Its poses and its offsets on the gun kind's built-in actions apply by
/// themselves; its own actions are done with a Gun Holder's Do. Give one to a Gun with its Handling, or in the
/// Inspector.
/// </summary>
public readonly struct GunHandling : IEquatable<GunHandling>, IAsset<GunHandling>
{
	public static readonly GunHandling None = default;

	/// <summary>The handling's GUID, or away from the engine its path.</summary>
	private readonly string? key;

	private GunHandling(string key)
	{
		this.key = key;
	}

	public bool IsValid => !string.IsNullOrEmpty(key);

	internal string Key => key ?? string.Empty;

	string? IAsset<GunHandling>.AssetGuid => IsValid ? key : null;

	/// <summary>
	/// A Gun Handling by project path or GUID, for example "Guns/Rifle.gunhandling". Logs an error and returns None
	/// when the project has no such handling.
	/// </summary>
	public static GunHandling Load(string path)
	{
		if (string.IsNullOrEmpty(path))
		{
			return None;
		}

		var found = MotionParts.SetKey(path);
		if (found == null)
		{
			Log.Error($"The project has no Gun Handling \"{path}\".");
			return None;
		}
		return new GunHandling(found);
	}

	internal static GunHandling FromKey(string key)
	{
		return string.IsNullOrEmpty(key) ? None : new GunHandling(key);
	}

	public static bool operator ==(GunHandling left, GunHandling right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(GunHandling left, GunHandling right)
	{
		return !left.Equals(right);
	}

	public bool Equals(GunHandling other)
	{
		return Key == other.Key;
	}

	public override bool Equals(object? other)
	{
		return other is GunHandling handling && Equals(handling);
	}

	public override int GetHashCode()
	{
		return Key.GetHashCode();
	}

	public override string ToString()
	{
		return IsValid ? Key : "GunHandling(none)";
	}
}
