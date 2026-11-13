using System;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A texture pack: the pictures a game's blocks and materials are drawn with in place of their own, which whoever
/// plays the game picks. A field holds one of the project's, a <c>.texturepack</c> asset picked in the Inspector;
/// TexturePacks.All lists every one there is to wear, the engine's own and the player's too. Give one to
/// TexturePacks.Use to wear it.
/// <code>
/// public TexturePack HandDrawn;
/// ...
/// TexturePacks.Use(HandDrawn);
/// </code>
/// </summary>
public readonly struct TexturePack : IEquatable<TexturePack>, IAsset<TexturePack>
{
	public static readonly TexturePack None = default;

	/// <summary>Every block of a plain colour drawn as sixteen sharp pixels a side, each a shade of its own.</summary>
	public static readonly TexturePack PixelGrain = new("builtin:pixel_grain", "Pixel Grain");

	/// <summary>A fine, soft grain on every block of a plain colour.</summary>
	public static readonly TexturePack SoftGrain = new("builtin:soft_grain", "Soft Grain");

	private readonly string? reference;
	private readonly string? name;
	private readonly string? description;

	internal TexturePack(
		string reference,
		string name,
		string description = "",
		TexturePackSource source = TexturePackSource.BuiltIn)
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

	/// <summary>What a menu calls it: its file's or folder's name, or a built-in pack's own.</summary>
	public string Name => name ?? string.Empty;

	/// <summary>A line saying what it looks like, as its file gives it; empty for one that gives none.</summary>
	public string Description => description ?? string.Empty;

	/// <summary>Where it is from: the engine, the game or the player.</summary>
	public TexturePackSource Source { get; }

	string? IAsset<TexturePack>.AssetGuid => IsSet ? NativeSaves.Key(NativeSaves.KeyReference, 0, Reference) : null;

	/// <summary>
	/// The texture pack at a project path or GUID, as "Packs/Hand Drawn.texturepack". Nothing is read until it is
	/// worn.
	/// </summary>
	public static TexturePack Load(string path)
	{
		var file = System.IO.Path.GetFileNameWithoutExtension(path);
		return new TexturePack(path, file, string.Empty, TexturePackSource.Game);
	}

	public static bool operator ==(TexturePack left, TexturePack right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(TexturePack left, TexturePack right)
	{
		return !left.Equals(right);
	}

	public bool Equals(TexturePack other)
	{
		return Reference == other.Reference;
	}

	public override bool Equals(object? other)
	{
		return other is TexturePack pack && Equals(pack);
	}

	public override int GetHashCode()
	{
		return Reference.GetHashCode();
	}

	public override string ToString()
	{
		return IsSet ? Name : "no texture pack";
	}
}
