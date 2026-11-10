using System;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A picture of the project, as a field holds it: picked in the Inspector, or dragged there from the Project panel.
/// It names the picture by its ID, so renaming or moving the file keeps the field pointing at it. An item's icon is
/// one.
/// <code>
/// public Texture Icon;
/// </code>
/// </summary>
public readonly struct Texture : IEquatable<Texture>, IAsset<Texture>
{
	public static readonly Texture None = default;

	private readonly string? reference;

	private Texture(string reference)
	{
		this.reference = reference;
	}

	/// <summary>Whether it names a picture.</summary>
	public bool IsSet => !string.IsNullOrEmpty(reference);

	/// <summary>The picture's GUID or project path, as the scene or code gave it.</summary>
	public string Reference => reference ?? string.Empty;

	string? IAsset<Texture>.AssetGuid => IsSet ? NativeSaves.Key(NativeSaves.KeyReference, 0, Reference) : null;

	/// <summary>The picture at a project path or GUID, as "Icons/Pickaxe.png". Nothing is read here, so this never fails.</summary>
	public static Texture Load(string path)
	{
		return new Texture(path);
	}

	public static bool operator ==(Texture left, Texture right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(Texture left, Texture right)
	{
		return !left.Equals(right);
	}

	public bool Equals(Texture other)
	{
		return Reference == other.Reference;
	}

	public override bool Equals(object? other)
	{
		return other is Texture texture && Equals(texture);
	}

	public override int GetHashCode()
	{
		return Reference.GetHashCode();
	}

	public override string ToString()
	{
		return IsSet ? Reference : "Texture(none)";
	}
}
