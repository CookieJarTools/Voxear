using System;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A look pack: a few lighting settings laid over whatever lighting a level has, such as a warmer sun, clouds built
/// of cubes or more glow, which whoever plays the game picks. A field holds one of the project's, a
/// <c>.lookpack</c> asset picked in the Inspector; LookPacks.All lists every one there is to wear, the engine's own
/// and the player's too. Give one to LookPacks.Use to wear it.
/// <code>
/// public LookPack Cosy;
/// ...
/// LookPacks.Use(Cosy);
/// </code>
/// </summary>
public readonly struct LookPack : IEquatable<LookPack>, IAsset<LookPack>
{
	public static readonly LookPack None = default;

	/// <summary>Soft sunlight, blue shade, a little glow and rich, clean colour.</summary>
	public static readonly LookPack Sunlit = new("builtin:sunlit", "Sunlit");

	/// <summary>The level's clouds built of big soft cubes, their square shadows crossing the ground.</summary>
	public static readonly LookPack CubeClouds = new("builtin:cube_clouds", "Cube Clouds");

	/// <summary>Every block's faces cut into little cubes, with clouds built of cubes to match.</summary>
	public static readonly LookPack LittleCubes = new("builtin:little_cubes", "Little Cubes");

	/// <summary>Rich colour and soft shapes, as in a picture book.</summary>
	public static readonly LookPack Painted = new("builtin:painted", "Painted");

	/// <summary>Faded colour, deep contrast, dark corners and the fringing of an old lens.</summary>
	public static readonly LookPack OldFilm = new("builtin:old_film", "Old Film");

	/// <summary>Nothing over the picture: no glow, no mist, no dark corners, and plain faces on every block.</summary>
	public static readonly LookPack Plain = new("builtin:plain", "Plain");

	private readonly string? reference;
	private readonly string? name;
	private readonly string? description;

	internal LookPack(
		string reference,
		string name,
		string description = "",
		LookPackSource source = LookPackSource.BuiltIn)
	{
		this.reference = reference;
		this.name = name;
		this.description = description;
		Source = source;
	}

	/// <summary>Whether it names a pack.</summary>
	public bool IsSet => !string.IsNullOrEmpty(reference);

	/// <summary>What names it to the engine: a built-in pack's name, an asset's GUID or path, or a file.</summary>
	public string Reference => reference ?? string.Empty;

	/// <summary>What a menu calls it: its file's name, or a built-in pack's own.</summary>
	public string Name => name ?? string.Empty;

	/// <summary>A line saying what it looks like, as its file gives it; empty for one that gives none.</summary>
	public string Description => description ?? string.Empty;

	/// <summary>Where it is from: the engine, the game or the player.</summary>
	public LookPackSource Source { get; }

	string? IAsset<LookPack>.AssetGuid => IsSet ? NativeSaves.Key(NativeSaves.KeyReference, 0, Reference) : null;

	/// <summary>
	/// The look pack at a project path or GUID, as "Looks/Cosy.lookpack". Nothing is read until it is worn.
	/// </summary>
	public static LookPack Load(string path)
	{
		var file = System.IO.Path.GetFileNameWithoutExtension(path);
		return new LookPack(path, file, string.Empty, LookPackSource.Game);
	}

	public static bool operator ==(LookPack left, LookPack right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(LookPack left, LookPack right)
	{
		return !left.Equals(right);
	}

	public bool Equals(LookPack other)
	{
		return Reference == other.Reference;
	}

	public override bool Equals(object? other)
	{
		return other is LookPack pack && Equals(pack);
	}

	public override int GetHashCode()
	{
		return Reference.GetHashCode();
	}

	public override string ToString()
	{
		return IsSet ? Name : "no look pack";
	}
}
