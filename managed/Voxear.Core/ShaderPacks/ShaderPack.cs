using System;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A shader pack: programs of its own that change how the game is lit and finished, such as shafts of sun through
/// leaves, warmer light or a sky of another colour, which whoever plays the game picks. A field holds one of the
/// project's, a <c>.shaderpack</c> folder's <c>pack.toml</c> picked in the Inspector; ShaderPacks.All lists every one
/// there is to wear, the engine's own and the player's too. Give one to ShaderPacks.Use to wear it.
/// <code>
/// public ShaderPack Dusk;
/// ...
/// ShaderPacks.Use(Dusk);
/// </code>
/// </summary>
public readonly struct ShaderPack : IEquatable<ShaderPack>, IAsset<ShaderPack>
{
	public static readonly ShaderPack None = default;

	/// <summary>
	/// Long warm evenings: warm sun and cool shade, leaves glowing against a low sun, a golden path of sun on the
	/// water, a gold sky, shafts of sun through the leaves and a warm grade.
	/// </summary>
	public static readonly ShaderPack Evening = new("builtin:evening", "Evening");

	private readonly string? reference;
	private readonly string? name;
	private readonly string? description;

	internal ShaderPack(
		string reference,
		string name,
		string description = "",
		ShaderPackSource source = ShaderPackSource.BuiltIn)
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

	/// <summary>A line saying what it looks like, as its pack.toml gives it; empty for one that gives none.</summary>
	public string Description => description ?? string.Empty;

	/// <summary>Where it is from: the engine, the game or the player.</summary>
	public ShaderPackSource Source { get; }

	string? IAsset<ShaderPack>.AssetGuid => IsSet ? NativeSaves.Key(NativeSaves.KeyReference, 0, Reference) : null;

	/// <summary>
	/// The shader pack at a project path or GUID, as "Packs/Dusk.shaderpack/pack.toml". Nothing is read until it is
	/// worn.
	/// </summary>
	public static ShaderPack Load(string path)
	{
		var folder = System.IO.Path.GetDirectoryName(path);
		var name = System.IO.Path.GetFileNameWithoutExtension(string.IsNullOrEmpty(folder) ? path : folder);
		return new ShaderPack(path, name, string.Empty, ShaderPackSource.Game);
	}

	public static bool operator ==(ShaderPack left, ShaderPack right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(ShaderPack left, ShaderPack right)
	{
		return !left.Equals(right);
	}

	public bool Equals(ShaderPack other)
	{
		return Reference == other.Reference;
	}

	public override bool Equals(object? other)
	{
		return other is ShaderPack pack && Equals(pack);
	}

	public override int GetHashCode()
	{
		return Reference.GetHashCode();
	}

	public override string ToString()
	{
		return IsSet ? Name : "no shader pack";
	}
}
