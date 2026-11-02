using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Joint, which holds its Rigidbody to another Obj's, or to where it is in the world when it names none, as
/// Unity's joints do. Get it with Obj.Joint, or give an Obj one with Obj.AddJoint. Its Anchor, from the Obj's origin,
/// is a hinge's or a ball's point, and the point a Distance joint's Obj hangs from; a hinge turns about Axis, in the
/// Obj's space. A change remakes the joint where its Objs are then.
/// <code>
/// var door = Obj.AddJoint(JointType.Hinge);
/// door.Anchor = new Vector3(-0.5f, 0f, 0f);
/// door.SetLimits(0f, 90f);
/// </code>
/// </summary>
public readonly struct Joint : IEquatable<Joint>, IObjComponent
{
	public static readonly Joint None = default;

	internal Joint(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj it holds.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Joint, or is gone.</summary>
	public bool IsValid => Obj.IsValid && NativePhysics.Flag(Obj, NativePhysics.JointHas);

	public JointType Type
	{
		get { return (JointType)(int)NativePhysics.Number(Obj, NativePhysics.JointType, 0f); }
		set { Set(NativePhysics.JointType, new Vector4((int)value, 0f, 0f, 0f)); }
	}

	/// <summary>The Rigidbody it holds to, which is not valid when it holds to the world.</summary>
	public Rigidbody ConnectedBody
	{
		get { return NativePhysics.Connected(Obj).Rigidbody; }
		set
		{
			if (!NativePhysics.Connect(Obj, value.Obj))
			{
				Missing();
			}
		}
	}

	/// <summary>A hinge's or a ball's point, or what a Distance joint hangs from, from the Obj's origin.</summary>
	public Vector3 Anchor
	{
		get { return NativePhysics.Vector(Obj, NativePhysics.JointAnchor, Vector3.Zero); }
		set { Set(NativePhysics.JointAnchor, new Vector4(value, 0f)); }
	}

	/// <summary>The axis a hinge turns about, in the Obj's space.</summary>
	public Vector3 Axis
	{
		get { return NativePhysics.Vector(Obj, NativePhysics.JointAxis, Vector3.UnitY); }
		set { Set(NativePhysics.JointAxis, new Vector4(value, 0f)); }
	}

	/// <summary>Whether it is held within limits: SetLimits gives them, and ClearLimits takes them away.</summary>
	public bool UseLimits => NativePhysics.Get(Obj, NativePhysics.JointLimits, out var limits) && limits.Z != 0f;

	/// <summary>A hinge's least angle in degrees, or a Distance joint's least length in metres.</summary>
	public float Min => NativePhysics.Get(Obj, NativePhysics.JointLimits, out var limits) ? limits.X : 0f;

	/// <summary>A hinge's greatest angle in degrees, or a Distance joint's greatest length in metres.</summary>
	public float Max => NativePhysics.Get(Obj, NativePhysics.JointLimits, out var limits) ? limits.Y : 0f;

	/// <summary>Holds a hinge between two angles, in degrees, or a Distance joint between two lengths, in metres.</summary>
	public void SetLimits(float min, float max)
	{
		Set(NativePhysics.JointLimits, new Vector4(min, max, 1f, 0f));
	}

	public void ClearLimits()
	{
		Set(NativePhysics.JointLimits, Vector4.Zero);
	}

	/// <summary>Takes the Joint off its Obj, which is then free.</summary>
	public void Remove()
	{
		NativePhysics.Set(Obj, NativePhysics.JointRemove, Vector4.Zero);
	}

	public static bool operator ==(Joint left, Joint right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(Joint left, Joint right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(Joint other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is Joint joint && Equals(joint);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Joint";
	}

	private void Set(int what, Vector4 value)
	{
		if (!NativePhysics.Set(Obj, what, value))
		{
			Missing();
		}
	}

	private void Missing()
	{
		if (Native.IsConnected)
		{
			Log.Error($"{Obj} has no Joint; give it one with AddJoint.");
		}
	}
}
