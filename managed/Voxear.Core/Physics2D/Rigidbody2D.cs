using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Rigidbody2D, which hands the Obj to 2D physics, as Unity's Rigidbody2D: it moves across the XY plane and
/// turns about Z, keeping its Z. Get it with Obj.Rigidbody2D, or give an Obj one with Obj.AddRigidbody2D. It needs a
/// Collider2D beside it to have a shape.
/// </summary>
public readonly struct Rigidbody2D : IEquatable<Rigidbody2D>, IObjComponent
{
	private const float Degrees = 180f / MathF.PI;

	public static readonly Rigidbody2D None = default;

	internal Rigidbody2D(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj physics moves.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Rigidbody2D, or is gone.</summary>
	public bool IsValid => Obj.IsValid && NativePhysics.Flag(Obj, NativePhysics.Body2DHas);

	/// <summary>How heavy it is, in kilograms.</summary>
	public float Mass
	{
		get { return NativePhysics.Number(Obj, NativePhysics.Body2DMass, 1f); }
		set { Set(NativePhysics.Body2DMass, value); }
	}

	/// <summary>Whether the game moves it rather than physics, pushing whatever is in its way: a moving platform.</summary>
	public bool IsKinematic
	{
		get { return NativePhysics.Flag(Obj, NativePhysics.Body2DKinematic); }
		set { Set(NativePhysics.Body2DKinematic, value ? 1f : 0f); }
	}

	/// <summary>How strongly gravity pulls it: 0 floats, 1 falls as everything else does, 2 twice as hard.</summary>
	public float GravityScale
	{
		get { return NativePhysics.Number(Obj, NativePhysics.Body2DGravityScale, 1f); }
		set { Set(NativePhysics.Body2DGravityScale, value); }
	}

	/// <summary>How quickly it slows down as it moves.</summary>
	public float Drag
	{
		get { return NativePhysics.Number(Obj, NativePhysics.Body2DDrag, 0f); }
		set { Set(NativePhysics.Body2DDrag, value); }
	}

	/// <summary>How quickly it stops spinning.</summary>
	public float AngularDrag
	{
		get { return NativePhysics.Number(Obj, NativePhysics.Body2DAngularDrag, 0.05f); }
		set { Set(NativePhysics.Body2DAngularDrag, value); }
	}

	/// <summary>Whether it never turns, as a platformer's character should not.</summary>
	public bool FixedRotation
	{
		get { return NativePhysics.Flag(Obj, NativePhysics.Body2DFixedRotation); }
		set { Set(NativePhysics.Body2DFixedRotation, value ? 1f : 0f); }
	}

	/// <summary>Whether its whole path is checked each step, so that something fast cannot pass through a thin wall.</summary>
	public bool IsContinuous
	{
		get { return NativePhysics.Flag(Obj, NativePhysics.Body2DContinuous); }
		set { Set(NativePhysics.Body2DContinuous, value ? 1f : 0f); }
	}

	/// <summary>How fast it moves, and which way, in metres per second.</summary>
	public Vector2 Velocity
	{
		get { return NativePhysics.Vector2(Obj, NativePhysics.Body2DVelocity, Vector2.Zero); }
		set
		{
			if (!NativePhysics.Set(Obj, NativePhysics.Body2DVelocity, new Vector4(value, 0f, 0f)))
			{
				Missing();
			}
		}
	}

	/// <summary>How fast it turns anticlockwise, in degrees per second, as in Unity.</summary>
	public float AngularVelocity
	{
		get { return NativePhysics.Number(Obj, NativePhysics.Body2DAngularVelocity, 0f) * Degrees; }
		set { Set(NativePhysics.Body2DAngularVelocity, value / Degrees); }
	}

	/// <summary>Whether it has come to rest and sleeps, costing nothing until something moves it.</summary>
	public bool IsSleeping => IsValid && !NativePhysics.Flag(Obj, NativePhysics.Body2DAwake);

	public void WakeUp()
	{
		Set(NativePhysics.Body2DAwake, 1f);
	}

	public void Sleep()
	{
		Set(NativePhysics.Body2DAwake, 0f);
	}

	/// <summary>Pushes it through its middle, so that it moves without turning.</summary>
	public void AddForce(Vector2 force, ForceMode2D mode = ForceMode2D.Force)
	{
		Push(mode == ForceMode2D.Impulse ? NativePhysics.Push2DImpulse : NativePhysics.Push2DForce, force);
	}

	/// <summary>Pushes it at a point in the XY plane, so that it turns as well, unless the point is its middle.</summary>
	public void AddForceAtPosition(Vector2 force, Vector2 position, ForceMode2D mode = ForceMode2D.Force)
	{
		var kind = mode == ForceMode2D.Impulse ? NativePhysics.Push2DImpulse : NativePhysics.Push2DForce;
		if (!NativePhysics.PushAt(Obj, kind, new Vector3(force, 0f), new Vector3(position, 0f)))
		{
			Missing();
		}
	}

	/// <summary>Turns it anticlockwise, by a torque in newton-metres, or a kick of one with Impulse.</summary>
	public void AddTorque(float torque, ForceMode2D mode = ForceMode2D.Force)
	{
		var kind = mode == ForceMode2D.Impulse ? NativePhysics.Push2DAngularImpulse : NativePhysics.Push2DTorque;
		Push(kind, new Vector2(torque, 0f));
	}

	/// <summary>Takes the Rigidbody2D off its Obj, which then stays where it is.</summary>
	public void Remove()
	{
		NativePhysics.Set(Obj, NativePhysics.Body2DRemove, Vector4.Zero);
	}

	public static bool operator ==(Rigidbody2D left, Rigidbody2D right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(Rigidbody2D left, Rigidbody2D right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(Rigidbody2D other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is Rigidbody2D body && Equals(body);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Rigidbody2D";
	}

	private void Push(int kind, Vector2 amount)
	{
		if (!NativePhysics.Push(Obj, kind, new Vector3(amount, 0f)))
		{
			Missing();
		}
	}

	private void Set(int what, float value)
	{
		if (!NativePhysics.Set(Obj, what, new Vector4(value, 0f, 0f, 0f)))
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
			? $"{Obj}'s Rigidbody2D has no Collider2D beside it, so physics cannot move it; give it one with AddCollider2D."
			: $"{Obj} has no Rigidbody2D; give it one with AddRigidbody2D.");
	}
}
