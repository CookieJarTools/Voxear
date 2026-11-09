using System;
using System.Collections.Generic;
using System.Linq;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The bones of the model an Obj draws, posed as animation and scripts have them this frame. Read a bone anywhere;
/// set one in PoseUpdate, after animation has posed it, and what is set shows this frame:
/// <code>
/// protected override void PoseUpdate()
/// {
/// 	var head = Obj.Skeleton.Find("Head");
/// 	head.Rotation = Quaternion.CreateFromAxisAngle(Vector3.UnitY, 0.3f) * head.Rotation;
/// }
/// </code>
/// </summary>
public readonly struct Skeleton : IEquatable<Skeleton>
{
	public static readonly Skeleton None = default;

	internal Skeleton(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj that draws the model.</summary>
	public Obj Obj { get; }

	/// <summary>False when the Obj draws no model with bones, or is gone.</summary>
	public bool IsValid => BoneCount > 0;

	/// <summary>How many bones the model has.</summary>
	public int BoneCount => Obj.IsValid ? (int)NativeComponent.Number(NativeComponent.Skeleton, Obj, Bone.CountValue, 0f) : 0;

	/// <summary>A bone by its number, parents before their children; <see cref="Bone.None"/> past the last.</summary>
	public Bone this[int index] => index >= 0 && index < BoneCount ? new Bone(Obj, index) : Bone.None;

	/// <summary>Every bone, parents before their children.</summary>
	public IEnumerable<Bone> Bones
	{
		get
		{
			var obj = Obj;
			return Enumerable.Range(0, BoneCount).Select(index => new Bone(obj, index));
		}
	}

	/// <summary>The bone with this name, matched exactly or else whatever its case; <see cref="Bone.None"/> when none is.</summary>
	public Bone Find(string name)
	{
		var answer = NativeComponent.Ask(NativeComponent.Skeleton, Obj, Bone.FindText, name);
		return int.TryParse(answer, out var index) && index >= 0 ? new Bone(Obj, index) : Bone.None;
	}

	public static bool operator ==(Skeleton left, Skeleton right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(Skeleton left, Skeleton right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(Skeleton other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is Skeleton skeleton && Equals(skeleton);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Skeleton";
	}
}
