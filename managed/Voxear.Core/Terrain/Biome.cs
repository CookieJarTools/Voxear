using System;

namespace Voxear;

/// <summary>
/// A biome: a .biome file, a kind of country with the climate it lies in and the blocks its ground is made of. A
/// script holds one in a field, picked in the Inspector, or asks which one a place is with <see cref="Biomes.At"/>.
/// <code>
/// public Biome Desert;
/// ...
/// if (Biomes.At(player.Position) == Desert)
/// {
///     thirst.Rate = 2f;
/// }
/// </code>
/// </summary>
public readonly struct Biome : IEquatable<Biome>, IAsset<Biome>
{
	public static readonly Biome None = default;

	/// <summary>The biome's GUID, or away from the engine its path.</summary>
	private readonly string? key;

	private Biome(string key)
	{
		this.key = key;
	}

	/// <summary>Whether it is a biome: false for none, as where no world with biomes reaches.</summary>
	public bool IsValid => !string.IsNullOrEmpty(key);

	internal string Key => key ?? string.Empty;

	string? IAsset<Biome>.AssetGuid => IsValid ? key : null;

	/// <summary>What it is called: its file's name.</summary>
	public string Name => IsValid ? BiomeInfos.Of(key!)?.Name ?? string.Empty : string.Empty;

	/// <summary>Its colour on maps and previews.</summary>
	public Color Color => IsValid ? BiomeInfos.Of(key!)?.Color ?? Color.Clear : Color.Clear;

	/// <summary>
	/// A biome by project path or GUID, for example "Biomes/Desert.biome". Logs an error and returns None when the
	/// project has no such biome.
	/// </summary>
	public static Biome Load(string path)
	{
		if (string.IsNullOrEmpty(path))
		{
			return None;
		}

		var found = BiomeInfos.Key(path);
		if (found == null || BiomeInfos.Of(found) == null)
		{
			Log.Error($"The project has no biome \"{path}\".");
			return None;
		}
		return new Biome(found);
	}

	internal static Biome FromKey(string? key)
	{
		return string.IsNullOrEmpty(key) ? None : new Biome(key);
	}

	public static bool operator ==(Biome left, Biome right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(Biome left, Biome right)
	{
		return !left.Equals(right);
	}

	public bool Equals(Biome other)
	{
		return Key == other.Key;
	}

	public override bool Equals(object? other)
	{
		return other is Biome biome && Equals(biome);
	}

	public override int GetHashCode()
	{
		return Key.GetHashCode();
	}

	public override string ToString()
	{
		return IsValid ? Name : "None";
	}
}
