using System;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A Voxel Print of the project, as a field holds it: a <c>.voxprint</c>, a model made into voxels solid all through,
/// picked in the Inspector or dragged there from the Project panel. It names the print by its ID, so renaming or moving
/// the file keeps the field pointing at it. Print it into the level's endless Voxel Volumes with
/// <see cref="Voxels.Print(VoxelPrint, System.Numerics.Vector3, float)"/>:
/// <code>
/// public VoxelPrint SandbagWall;
/// ...
/// Voxels.Print(SandbagWall, hit.Point, turn: 90f);
/// </code>
/// </summary>
public readonly struct VoxelPrint : IEquatable<VoxelPrint>, IAsset<VoxelPrint>
{
	public static readonly VoxelPrint None = default;

	private readonly string? reference;

	private VoxelPrint(string reference)
	{
		this.reference = reference;
	}

	/// <summary>Whether it names a print.</summary>
	public bool IsSet => !string.IsNullOrEmpty(reference);

	/// <summary>The print's GUID or project path, as the scene or code gave it.</summary>
	public string Reference => reference ?? string.Empty;

	string? IAsset<VoxelPrint>.AssetGuid => IsSet ? NativeSaves.Key(NativeSaves.KeyReference, 0, Reference) : null;

	/// <summary>The print at a project path or GUID, as "Prints/Sandbag Wall.voxprint". Nothing is read here, so this never fails.</summary>
	public static VoxelPrint Load(string path)
	{
		return new VoxelPrint(path);
	}

	public static bool operator ==(VoxelPrint left, VoxelPrint right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(VoxelPrint left, VoxelPrint right)
	{
		return !left.Equals(right);
	}

	public bool Equals(VoxelPrint other)
	{
		return Reference == other.Reference;
	}

	public override bool Equals(object? other)
	{
		return other is VoxelPrint print && Equals(print);
	}

	public override int GetHashCode()
	{
		return Reference.GetHashCode();
	}

	public override string ToString()
	{
		return IsSet ? Reference : "no voxel print";
	}
}
