using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// One bone of a model's <see cref="Skeleton"/>, where animation and scripts have posed it this frame. Its local place
/// and turn are in its parent bone's space; its place and turn are in the world. Setting a bone carries the bones
/// under it with it. Set bones in PoseUpdate, after animation has posed them, or animation poses over what was set.
/// </summary>
public readonly struct Bone : IEquatable<Bone>
{
	// The values the engine's animation/bones.rs answers, in the low byte; the bone is the bytes above.
	internal const int CountValue = 0;
	internal const int FindText = 1;
	private const int ParentValue = 1;
	private const int LocalPositionValue = 2;
	private const int LocalRotationValue = 3;
	private const int PositionValue = 4;
	private const int RotationValue = 5;
	private const int RestPositionValue = 6;
	private const int RestRotationValue = 7;
	private const int NameText = 0;

	public static readonly Bone None = default;

	internal Bone(Obj obj, int index)
	{
		Obj = obj;
		Index = index;
	}

	/// <summary>The Obj that draws the bone's model.</summary>
	public Obj Obj { get; }

	/// <summary>The bone's number in its skeleton.</summary>
	public int Index { get; }

	/// <summary>False for <see cref="None"/>, or when its model or Obj is gone.</summary>
	public bool IsValid => Index >= 0 && Index < new Skeleton(Obj).BoneCount;

	/// <summary>Its name in the model.</summary>
	public string Name => NativeComponent.Text(NativeComponent.Skeleton, Obj, Index << 8 | NameText) ?? string.Empty;

	/// <summary>The bone it hangs from; <see cref="None"/> for a root.</summary>
	public Bone Parent
	{
		get
		{
			var parent = (int)Get(ParentValue).X;
			return parent >= 0 ? new Bone(Obj, parent) : None;
		}
	}

	/// <summary>Where it is in its parent bone's space, in metres.</summary>
	public Vector3 LocalPosition
	{
		get { return Point(LocalPositionValue); }
		set { Set(LocalPositionValue, new Vector4(value, 0f)); }
	}

	/// <summary>How it is turned in its parent bone's space.</summary>
	public Quaternion LocalRotation
	{
		get { return Turn(LocalRotationValue); }
		set { Set(LocalRotationValue, new Vector4(value.X, value.Y, value.Z, value.W)); }
	}

	/// <summary>Where it is in the world.</summary>
	public Vector3 Position
	{
		get { return Point(PositionValue); }
		set { Set(PositionValue, new Vector4(value, 0f)); }
	}

	/// <summary>How it is turned in the world.</summary>
	public Quaternion Rotation
	{
		get { return Turn(RotationValue); }
		set { Set(RotationValue, new Vector4(value.X, value.Y, value.Z, value.W)); }
	}

	/// <summary>Where it is in its parent bone's space at rest, as the model was made.</summary>
	public Vector3 RestPosition => Point(RestPositionValue);

	/// <summary>How it is turned in its parent bone's space at rest, as the model was made.</summary>
	public Quaternion RestRotation => Turn(RestRotationValue);

	public static bool operator ==(Bone left, Bone right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(Bone left, Bone right)
	{
		return !left.Equals(right);
	}

	public bool Equals(Bone other)
	{
		return Obj == other.Obj && Index == other.Index;
	}

	public override bool Equals(object? other)
	{
		return other is Bone bone && Equals(bone);
	}

	public override int GetHashCode()
	{
		return HashCode.Combine(Obj, Index);
	}

	public override string ToString()
	{
		return IsValid ? $"{Obj}'s bone {Name}" : "no bone";
	}

	private Vector4 Get(int what)
	{
		NativeComponent.Get(NativeComponent.Skeleton, Obj, Index << 8 | what, out var value);
		return value;
	}

	private Vector3 Point(int what)
	{
		var value = Get(what);
		return new Vector3(value.X, value.Y, value.Z);
	}

	private Quaternion Turn(int what)
	{
		var value = Get(what);
		return value == Vector4.Zero ? Quaternion.Identity : new Quaternion(value.X, value.Y, value.Z, value.W);
	}

	private void Set(int what, Vector4 value)
	{
		NativeComponent.Set(NativeComponent.Skeleton, Obj, Index << 8 | what, value);
	}
}
