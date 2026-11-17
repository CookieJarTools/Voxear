using System;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A model of the project, as a field holds it: a <c>.glb</c>, <c>.gltf</c>, <c>.fbx</c> or <c>.boxmodel</c>, picked in
/// the Inspector or dragged there from the Project panel. It names the model by its ID, so renaming or moving the file
/// keeps the field pointing at it. Give it to an Obj's Model Renderer to have the Obj draw it:
/// <code>
/// public ModelAsset LongHair;
/// ...
/// hair.Model.Model = LongHair;
/// </code>
/// </summary>
public readonly struct ModelAsset : IEquatable<ModelAsset>, IAsset<ModelAsset>
{
	public static readonly ModelAsset None = default;

	private readonly string? reference;

	private ModelAsset(string reference)
	{
		this.reference = reference;
	}

	/// <summary>Whether it names a model.</summary>
	public bool IsSet => !string.IsNullOrEmpty(reference);

	/// <summary>The model's GUID or project path, as the scene or code gave it.</summary>
	public string Reference => reference ?? string.Empty;

	string? IAsset<ModelAsset>.AssetGuid => IsSet ? NativeSaves.Key(NativeSaves.KeyReference, 0, Reference) : null;

	/// <summary>The model at a project path or GUID, as "Characters/Hair.boxmodel". Nothing is read here, so this never fails.</summary>
	public static ModelAsset Load(string path)
	{
		return new ModelAsset(path);
	}

	public static bool operator ==(ModelAsset left, ModelAsset right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(ModelAsset left, ModelAsset right)
	{
		return !left.Equals(right);
	}

	public bool Equals(ModelAsset other)
	{
		return Reference == other.Reference;
	}

	public override bool Equals(object? other)
	{
		return other is ModelAsset asset && Equals(asset);
	}

	public override int GetHashCode()
	{
		return Reference.GetHashCode();
	}

	public override string ToString()
	{
		return IsSet ? Reference : "no model";
	}
}
