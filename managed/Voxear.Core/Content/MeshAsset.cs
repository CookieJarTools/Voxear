using System;

namespace Voxear;

/// <summary>
/// A mesh, as a Dry Area keeps water out of one: one every project has, such as MeshAsset.Cube, or a mesh of one of the
/// project's models, with MeshAsset.Of.
/// <code>
/// hold.Mesh = MeshAsset.Of(HullModel);
/// </code>
/// </summary>
public readonly struct MeshAsset : IEquatable<MeshAsset>
{
	private const string Builtin = "builtin:";

	public static readonly MeshAsset None = default;

	/// <summary>The mesh as files name it: a built-in one, or a model's GUID and which of its meshes.</summary>
	private readonly string? reference;

	private MeshAsset(string reference)
	{
		this.reference = reference;
	}

	/// <summary>A cube a metre a side.</summary>
	public static MeshAsset Cube => new(Builtin + "cube");

	/// <summary>A ball a metre across.</summary>
	public static MeshAsset Sphere => new(Builtin + "sphere");

	/// <summary>A flat square a metre a side, facing up.</summary>
	public static MeshAsset Plane => new(Builtin + "plane");

	/// <summary>Whether it names a mesh.</summary>
	public bool IsSet => !string.IsNullOrEmpty(reference);

	internal string Reference => reference ?? string.Empty;

	/// <summary>A mesh of one of the project's models: its first, or the one at <paramref name="mesh"/>.</summary>
	public static MeshAsset Of(ModelAsset model, int mesh = 0)
	{
		IAsset<ModelAsset> asset = model;
		var guid = asset.AssetGuid ?? model.Reference;
		return model.IsSet ? new MeshAsset($"{guid}#mesh{mesh}.0") : None;
	}

	internal static MeshAsset FromReference(string? reference)
	{
		return string.IsNullOrEmpty(reference) ? None : new MeshAsset(reference);
	}

	public static bool operator ==(MeshAsset left, MeshAsset right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(MeshAsset left, MeshAsset right)
	{
		return !left.Equals(right);
	}

	public bool Equals(MeshAsset other)
	{
		return Reference == other.Reference;
	}

	public override bool Equals(object? other)
	{
		return other is MeshAsset mesh && Equals(mesh);
	}

	public override int GetHashCode()
	{
		return Reference.GetHashCode();
	}

	public override string ToString()
	{
		return IsSet ? Reference : "MeshAsset(none)";
	}
}
