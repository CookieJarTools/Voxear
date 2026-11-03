using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Collider, the shape other things bump into, as Unity's Collider. Get it with Obj.Collider, or give an
/// Obj one with Obj.AddCollider. On its own, the Obj stays where it is put; a Rigidbody beside it hands it to
/// physics. A trigger pushes nothing, and tells behaviours what enters and leaves it instead.
/// <para>
/// The shape grows with its Obj's scale. A sphere or a capsule cannot stretch one way only, so it takes the largest
/// of the scales across it.
/// </para>
/// </summary>
public readonly struct Collider : IEquatable<Collider>, IObjComponent
{
	public static readonly Collider None = default;

	internal Collider(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj it is the shape of.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Collider, or is gone.</summary>
	public bool IsValid => Obj.IsValid && NativePhysics.Flag(Obj, NativePhysics.ColliderHas);

	public ColliderShape Shape
	{
		get { return (ColliderShape)(int)NativePhysics.Number(Obj, NativePhysics.ColliderShape, 0f); }
		set { Set(NativePhysics.ColliderShape, new Vector4((int)value, 0f, 0f, 0f)); }
	}

	/// <summary>Where the middle of the shape is, from the Obj's origin.</summary>
	public Vector3 Center
	{
		get { return NativePhysics.Vector(Obj, NativePhysics.ColliderCenter, Vector3.Zero); }
		set { Set(NativePhysics.ColliderCenter, new Vector4(value, 0f)); }
	}

	/// <summary>A box's width, height and depth.</summary>
	public Vector3 Size
	{
		get { return NativePhysics.Vector(Obj, NativePhysics.ColliderSize, Vector3.One); }
		set { Set(NativePhysics.ColliderSize, new Vector4(value, 0f)); }
	}

	/// <summary>A sphere's or a capsule's radius.</summary>
	public float Radius
	{
		get { return NativePhysics.Number(Obj, NativePhysics.ColliderRadius, 0.5f); }
		set { Set(NativePhysics.ColliderRadius, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>A capsule's height, from end to end.</summary>
	public float Height
	{
		get { return NativePhysics.Number(Obj, NativePhysics.ColliderHeight, 2f); }
		set { Set(NativePhysics.ColliderHeight, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>
	/// Whether a Mesh collider is the convex hull round its mesh rather than its triangles. One with a Rigidbody that
	/// physics moves is always its hull.
	/// </summary>
	public bool Convex
	{
		get { return NativePhysics.Flag(Obj, NativePhysics.ColliderConvex); }
		set { Set(NativePhysics.ColliderConvex, new Vector4(value ? 1f : 0f, 0f, 0f, 0f)); }
	}

	/// <summary>Whether it is a zone that notices what passes through it, rather than something solid.</summary>
	public bool IsTrigger
	{
		get { return NativePhysics.Flag(Obj, NativePhysics.ColliderTrigger); }
		set { Set(NativePhysics.ColliderTrigger, new Vector4(value ? 1f : 0f, 0f, 0f, 0f)); }
	}

	/// <summary>How much it grips what slides on it, from 0 for ice.</summary>
	public float Friction
	{
		get { return NativePhysics.Number(Obj, NativePhysics.ColliderFriction, 0.6f); }
		set { Set(NativePhysics.ColliderFriction, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>How much things bounce off it, from 0 to 1.</summary>
	public float Bounciness
	{
		get { return NativePhysics.Number(Obj, NativePhysics.ColliderBounciness, 0f); }
		set { Set(NativePhysics.ColliderBounciness, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>
	/// What it is made of, to sound: walls of carpet soak up more of a sound and glass lets more through than brick,
	/// for Audio Sources with Occlusion on. A collider with a Rigidbody, or a trigger, is no wall to sound.
	/// </summary>
	public AcousticMaterial AcousticMaterial
	{
		get { return (AcousticMaterial)(int)NativePhysics.Number(Obj, NativePhysics.ColliderAcoustic, 0f); }
		set { Set(NativePhysics.ColliderAcoustic, new Vector4((int)value, 0f, 0f, 0f)); }
	}

	/// <summary>The Rigidbody beside it, which is not valid when physics does not move its Obj.</summary>
	public Rigidbody AttachedRigidbody => Obj.Rigidbody;

	/// <summary>Takes the Collider off its Obj.</summary>
	public void Remove()
	{
		NativePhysics.Set(Obj, NativePhysics.ColliderRemove, Vector4.Zero);
	}

	public static bool operator ==(Collider left, Collider right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(Collider left, Collider right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(Collider other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is Collider collider && Equals(collider);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Collider";
	}

	private void Set(int what, Vector4 value)
	{
		if (!NativePhysics.Set(Obj, what, value) && Native.IsConnected)
		{
			Log.Error($"{Obj} has no Collider; give it one with AddCollider.");
		}
	}
}
