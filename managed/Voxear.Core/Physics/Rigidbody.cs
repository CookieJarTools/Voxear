using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Rigidbody, which hands the Obj to physics, as Unity's Rigidbody. Get it with Obj.Rigidbody, or give an
/// Obj one with Obj.AddRigidbody. It needs a Collider beside it to have a shape.
/// <para>
/// Physics moves bodies in fixed steps, straight after FixedUpdate, so forces belong in FixedUpdate. A force spreads
/// over the next step; an impulse or a new velocity takes effect at once. Setting the Obj's position moves the body
/// there, as in Unity.
/// </para>
/// </summary>
public readonly struct Rigidbody : IEquatable<Rigidbody>, IObjComponent
{
	public static readonly Rigidbody None = default;

	internal Rigidbody(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj physics moves.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Rigidbody, or is gone.</summary>
	public bool IsValid => Obj.IsValid && NativePhysics.Flag(Obj, NativePhysics.BodyHas);

	/// <summary>How heavy it is, in kilograms.</summary>
	public float Mass
	{
		get { return NativePhysics.Number(Obj, NativePhysics.BodyMass, 1f); }
		set { Set(NativePhysics.BodyMass, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>Whether the game moves it rather than physics, pushing whatever is in its way: a lift or a door.</summary>
	public bool IsKinematic
	{
		get { return NativePhysics.Flag(Obj, NativePhysics.BodyKinematic); }
		set { Set(NativePhysics.BodyKinematic, new Vector4(value ? 1f : 0f, 0f, 0f, 0f)); }
	}

	/// <summary>Whether it falls.</summary>
	public bool UseGravity
	{
		get { return NativePhysics.Flag(Obj, NativePhysics.BodyGravity); }
		set { Set(NativePhysics.BodyGravity, new Vector4(value ? 1f : 0f, 0f, 0f, 0f)); }
	}

	/// <summary>How quickly it slows down as it moves through the air.</summary>
	public float Drag
	{
		get { return NativePhysics.Number(Obj, NativePhysics.BodyDrag, 0f); }
		set { Set(NativePhysics.BodyDrag, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>How quickly it stops spinning.</summary>
	public float AngularDrag
	{
		get { return NativePhysics.Number(Obj, NativePhysics.BodyAngularDrag, 0.05f); }
		set { Set(NativePhysics.BodyAngularDrag, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>Whether its whole path is checked each step, so that something fast cannot pass through a thin wall.</summary>
	public bool IsContinuous
	{
		get { return NativePhysics.Flag(Obj, NativePhysics.BodyContinuous); }
		set { Set(NativePhysics.BodyContinuous, new Vector4(value ? 1f : 0f, 0f, 0f, 0f)); }
	}

	/// <summary>How fast it moves, and which way, in metres per second.</summary>
	public Vector3 Velocity
	{
		get { return NativePhysics.Vector(Obj, NativePhysics.BodyVelocity, Vector3.Zero); }
		set { Set(NativePhysics.BodyVelocity, new Vector4(value, 0f)); }
	}

	/// <summary>How fast it spins, about each axis, in radians per second.</summary>
	public Vector3 AngularVelocity
	{
		get { return NativePhysics.Vector(Obj, NativePhysics.BodyAngularVelocity, Vector3.Zero); }
		set { Set(NativePhysics.BodyAngularVelocity, new Vector4(value, 0f)); }
	}

	/// <summary>Whether it has come to rest and sleeps, costing nothing until something moves it.</summary>
	public bool IsSleeping => IsValid && !NativePhysics.Flag(Obj, NativePhysics.BodyAwake);

	public void WakeUp()
	{
		Set(NativePhysics.BodyAwake, Vector4.UnitX);
	}

	/// <summary>Puts it to sleep, where it stays until something hits it or a script moves it.</summary>
	public void Sleep()
	{
		Set(NativePhysics.BodyAwake, Vector4.Zero);
	}

	/// <summary>Pushes it through its middle, so that it moves without turning.</summary>
	public void AddForce(Vector3 force, ForceMode mode = ForceMode.Force)
	{
		if (!NativePhysics.Push(Obj, Kind(mode), force))
		{
			Missing();
		}
	}

	/// <summary>Pushes it at a point in the world, so that it turns as well, unless the point is its middle.</summary>
	public void AddForceAtPosition(Vector3 force, Vector3 position, ForceMode mode = ForceMode.Force)
	{
		if (!NativePhysics.PushAt(Obj, Kind(mode), force, position))
		{
			Missing();
		}
	}

	/// <summary>
	/// Turns it about the axis the torque points along. Force and Impulse are in newton-metres and newton-metre-seconds,
	/// and a heavier body turns less; Acceleration and VelocityChange change how fast it spins, in radians per second,
	/// whatever it weighs.
	/// </summary>
	public void AddTorque(Vector3 torque, ForceMode mode = ForceMode.Force)
	{
		if (!Native.IsConnected)
		{
			return;
		}

		switch (mode)
		{
			case ForceMode.Acceleration:
				AngularVelocity += torque * Time.FixedDelta;
				return;
			case ForceMode.VelocityChange:
				AngularVelocity += torque;
				return;
		}

		var kind = mode == ForceMode.Impulse ? NativePhysics.PushAngularImpulse : NativePhysics.PushTorque;
		if (!NativePhysics.Push(Obj, kind, torque))
		{
			Missing();
		}
	}

	/// <summary>
	/// Throws it away from an explosion at a point, as Unity's AddExplosionForce: the full force at the point, fading to
	/// nothing at the radius, from below the point by upwardsModifier metres, so that what it throws lifts.
	/// </summary>
	public void AddExplosionForce(float force, Vector3 position, float radius, float upwardsModifier = 0f, ForceMode mode = ForceMode.Force)
	{
		var center = Obj.WorldPosition;
		var from = position - new Vector3(0f, upwardsModifier, 0f);
		var away = center - from;
		var distance = away.Length();
		if (distance >= radius || radius <= 0f)
		{
			return;
		}

		var direction = distance > 1e-4f ? away / distance : Vector3.UnitY;
		AddForce(direction * (force * (1f - distance / radius)), mode);
	}

	/// <summary>Takes the Rigidbody off its Obj, which then stays where it is.</summary>
	public void Remove()
	{
		NativePhysics.Set(Obj, NativePhysics.BodyRemove, Vector4.Zero);
	}

	public static bool operator ==(Rigidbody left, Rigidbody right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(Rigidbody left, Rigidbody right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(Rigidbody other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is Rigidbody body && Equals(body);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Rigidbody";
	}

	private static int Kind(ForceMode mode)
	{
		return mode switch
		{
			ForceMode.Acceleration => NativePhysics.PushAcceleration,
			ForceMode.Impulse => NativePhysics.PushImpulse,
			ForceMode.VelocityChange => NativePhysics.PushVelocityChange,
			_ => NativePhysics.PushForce,
		};
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
		if (!Native.IsConnected)
		{
			return;
		}

		Log.Error(IsValid
			? $"{Obj}'s Rigidbody has no Collider beside it, so physics cannot move it; give it one with AddCollider."
			: $"{Obj} has no Rigidbody; give it one with AddRigidbody.");
	}
}
