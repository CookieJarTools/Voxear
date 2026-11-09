using System;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A material of the project, as a field holds it: picked in the Inspector, or dragged there from the Project panel.
/// It names the material by its ID, so renaming or moving the file keeps the field pointing at it. A terrain's
/// surfaces are known by their materials, so this is how a script names one: the dirt a crater lays on its walls, or
/// the mud a cart's wheels paint.
/// <code>
/// public MaterialAsset Mud;
/// ...
/// Terrain.Paint(wheel.Position, 0.6f, Mud);
/// </code>
/// </summary>
public readonly struct MaterialAsset : IEquatable<MaterialAsset>, IAsset<MaterialAsset>
{
	public static readonly MaterialAsset None = default;

	private readonly string? reference;

	private MaterialAsset(string reference)
	{
		this.reference = reference;
	}

	/// <summary>Whether it names a material.</summary>
	public bool IsSet => !string.IsNullOrEmpty(reference);

	/// <summary>The material's GUID or project path, as the scene or code gave it.</summary>
	public string Reference => reference ?? string.Empty;

	string? IAsset<MaterialAsset>.AssetGuid => IsSet ? NativeSaves.Key(NativeSaves.KeyReference, 0, Reference) : null;

	/// <summary>The material at a project path or GUID, as "Materials/Mud.material". Nothing is read here, so this never fails.</summary>
	public static MaterialAsset Load(string path)
	{
		return new MaterialAsset(path);
	}

	public static bool operator ==(MaterialAsset left, MaterialAsset right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(MaterialAsset left, MaterialAsset right)
	{
		return !left.Equals(right);
	}

	public bool Equals(MaterialAsset other)
	{
		return Reference == other.Reference;
	}

	public override bool Equals(object? other)
	{
		return other is MaterialAsset asset && Equals(asset);
	}

	public override int GetHashCode()
	{
		return Reference.GetHashCode();
	}

	public override string ToString()
	{
		return IsSet ? Reference : "no material";
	}
}
