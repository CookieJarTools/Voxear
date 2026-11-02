using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Collider2D, its flat shape in the XY plane for 2D physics, as Unity's Collider2D. Get it with
/// Obj.Collider2D, or give an Obj one with Obj.AddCollider2D. On its own, the Obj stays where it is put; a
/// Rigidbody2D beside it hands it to 2D physics. A trigger pushes nothing, and tells behaviours what enters and
/// leaves it instead.
/// </summary>
public readonly struct Collider2D : IEquatable<Collider2D>, IObjComponent
{
	public static readonly Collider2D None = default;

	internal Collider2D(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj it is the shape of.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Collider2D, or is gone.</summary>
	public bool IsValid => Obj.IsValid && NativePhysics.Flag(Obj, NativePhysics.Collider2DHas);

	public ColliderShape2D Shape
	{
		get { return (ColliderShape2D)(int)NativePhysics.Number(Obj, NativePhysics.Collider2DShape, 0f); }
		set { Set(NativePhysics.Collider2DShape, new Vector4((int)value, 0f, 0f, 0f)); }
	}

	/// <summary>Where the middle of the shape is, from the Obj's origin.</summary>
	public Vector2 Offset
	{
		get { return NativePhysics.Vector2(Obj, NativePhysics.Collider2DOffset, Vector2.Zero); }
		set { Set(NativePhysics.Collider2DOffset, new Vector4(value, 0f, 0f)); }
	}

	/// <summary>A box's width and height.</summary>
	public Vector2 Size
	{
		get { return NativePhysics.Vector2(Obj, NativePhysics.Collider2DSize, Vector2.One); }
		set { Set(NativePhysics.Collider2DSize, new Vector4(value, 0f, 0f)); }
	}

	/// <summary>A circle's or a capsule's radius.</summary>
	public float Radius
	{
		get { return NativePhysics.Number(Obj, NativePhysics.Collider2DRadius, 0.5f); }
		set { Set(NativePhysics.Collider2DRadius, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>A capsule's height, from end to end.</summary>
	public float Height
	{
		get { return NativePhysics.Number(Obj, NativePhysics.Collider2DHeight, 2f); }
		set { Set(NativePhysics.Collider2DHeight, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>Whether it is a zone that notices what passes through it, rather than something solid.</summary>
	public bool IsTrigger
	{
		get { return NativePhysics.Flag(Obj, NativePhysics.Collider2DTrigger); }
		set { Set(NativePhysics.Collider2DTrigger, new Vector4(value ? 1f : 0f, 0f, 0f, 0f)); }
	}

	/// <summary>How much it grips what slides on it, from 0 for ice.</summary>
	public float Friction
	{
		get { return NativePhysics.Number(Obj, NativePhysics.Collider2DFriction, 0.6f); }
		set { Set(NativePhysics.Collider2DFriction, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>How much things bounce off it, from 0 to 1.</summary>
	public float Bounciness
	{
		get { return NativePhysics.Number(Obj, NativePhysics.Collider2DBounciness, 0f); }
		set { Set(NativePhysics.Collider2DBounciness, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>The Rigidbody2D beside it, which is not valid when physics does not move its Obj.</summary>
	public Rigidbody2D AttachedRigidbody => Obj.Rigidbody2D;

	/// <summary>Takes the Collider2D off its Obj.</summary>
	public void Remove()
	{
		NativePhysics.Set(Obj, NativePhysics.Collider2DRemove, Vector4.Zero);
	}

	public static bool operator ==(Collider2D left, Collider2D right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(Collider2D left, Collider2D right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(Collider2D other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is Collider2D collider && Equals(collider);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Collider2D";
	}

	private void Set(int what, Vector4 value)
	{
		if (!NativePhysics.Set(Obj, what, value) && Native.IsConnected)
		{
			Log.Error($"{Obj} has no Collider2D; give it one with AddCollider2D.");
		}
	}
}
