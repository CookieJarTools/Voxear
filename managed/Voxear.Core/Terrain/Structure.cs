using System;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A structure: a .structure file, which a world of blocks' generator builds, as a house, a tree, a town or a
/// dungeon. A script holds one in a field, picked in the Inspector, to tell which kind of house of a town it is
/// looking at.
/// <code>
/// public Structure Smithy;
/// ...
/// foreach (var building in town.Buildings.Where(building => building.Structure == Smithy))
/// {
///     smiths.Add(new Smith(building.Door));
/// }
/// </code>
/// </summary>
public readonly struct Structure : IEquatable<Structure>, IAsset<Structure>
{
	public static readonly Structure None = default;

	/// <summary>The structure's GUID, or where the project does not know it, its reference as given.</summary>
	private readonly string? key;

	private Structure(string key)
	{
		this.key = key;
	}

	/// <summary>Whether it names a structure.</summary>
	public bool IsValid => !string.IsNullOrEmpty(key);

	internal string Key => key ?? string.Empty;

	string? IAsset<Structure>.AssetGuid => IsValid ? key : null;

	/// <summary>
	/// A structure by project path or GUID, as "Structures/Pieces/Cottage.structure". Logs an error and returns None
	/// when the project has no such file.
	/// </summary>
	public static Structure Load(string path)
	{
		if (string.IsNullOrEmpty(path))
		{
			return None;
		}

		var guid = NativeSaves.Key(NativeSaves.KeyReference, 0, path);
		if (guid == null && Native.IsConnected)
		{
			Log.Error($"The project has no structure \"{path}\".");
			return None;
		}
		return new Structure(guid ?? path);
	}

	internal static Structure FromKey(string? key)
	{
		return string.IsNullOrEmpty(key) ? None : new Structure(key);
	}

	public static bool operator ==(Structure left, Structure right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(Structure left, Structure right)
	{
		return !left.Equals(right);
	}

	public bool Equals(Structure other)
	{
		return Key == other.Key;
	}

	public override bool Equals(object? other)
	{
		return other is Structure structure && Equals(structure);
	}

	public override int GetHashCode()
	{
		return Key.GetHashCode();
	}

	public override string ToString()
	{
		return IsValid ? Key : "None";
	}
}
