using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Mob Motion: it animates the Obj's model from how the Obj moves, with no clips. Legs step by how far the
/// Obj has gone, arms swing against them, the body bobs and leans, and standing still it breathes. It goes by what
/// part of a body each of the model's bones moves, as a box model's bones say. A game only tells it what it cannot
/// see for itself: where to look, when to attack, and when it is hit. Get it with Obj.MobMotion, or give an Obj one
/// with Obj.AddMobMotion.
/// <code>
/// // A guard that watches the player and swings when they come near.
/// var motion = Obj.MobMotion;
/// motion.LookAt(player.Position + Vector3.UnitY * 1.6f);
/// if (Vector3.Distance(player.Position, Obj.Position) &lt; 2f)
/// {
/// 	motion.Attack();
/// }
/// </code>
/// </summary>
public readonly struct MobMotion : IEquatable<MobMotion>, IObjComponent
{
	public static readonly MobMotion None = default;

	internal MobMotion(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj whose model it animates.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Mob Motion, or is gone.</summary>
	public bool IsValid => Obj.IsValid && NativeMobMotion.Number(Obj, NativeMobMotion.Has, 0f) != 0f;

	/// <summary>Whether its head is turned to a point it was told to look at, rather than ahead.</summary>
	public bool IsLooking => NativeMobMotion.Get(Obj, NativeMobMotion.LookAt, out var value) && value.W != 0f;

	/// <summary>How far its legs swing as it walks, in degrees.</summary>
	public float LegSwing
	{
		get { return Number(NativeMobMotion.LegSwing); }
		set { Set(NativeMobMotion.LegSwing, value); }
	}

	/// <summary>How far its arms swing against its legs, in degrees.</summary>
	public float ArmSwing
	{
		get { return Number(NativeMobMotion.ArmSwing); }
		set { Set(NativeMobMotion.ArmSwing, value); }
	}

	/// <summary>How many metres one step covers, or 0 for what its legs' length gives.</summary>
	public float Stride
	{
		get { return Number(NativeMobMotion.Stride); }
		set { Set(NativeMobMotion.Stride, value); }
	}

	/// <summary>How much its body bobs and leans as it walks: 1 as made, 0 not at all.</summary>
	public float Bounce
	{
		get { return Number(NativeMobMotion.Bounce); }
		set { Set(NativeMobMotion.Bounce, value); }
	}

	/// <summary>How much it breathes and shifts standing still: 1 as made, 0 not at all.</summary>
	public float Idle
	{
		get { return Number(NativeMobMotion.Idle); }
		set { Set(NativeMobMotion.Idle, value); }
	}

	/// <summary>How fast its head turns to what it looks at: higher is quicker.</summary>
	public float LookSpeed
	{
		get { return Number(NativeMobMotion.LookSpeed); }
		set { Set(NativeMobMotion.LookSpeed, value); }
	}

	/// <summary>How far its head turns from straight ahead, in degrees.</summary>
	public float LookLimit
	{
		get { return Number(NativeMobMotion.LookLimit); }
		set { Set(NativeMobMotion.LookLimit, value); }
	}

	/// <summary>How much its tail and wings trail behind what its body does: 1 as made, 0 not at all.</summary>
	public float Lag
	{
		get { return Number(NativeMobMotion.Lag); }
		set { Set(NativeMobMotion.Lag, value); }
	}

	/// <summary>Turns its head to a point in the world, as far as its neck goes, until told otherwise.</summary>
	public void LookAt(Vector3 point)
	{
		Set(NativeMobMotion.LookAt, new Vector4(point, 1f));
	}

	/// <summary>Has it look ahead again.</summary>
	public void LookAhead()
	{
		Set(NativeMobMotion.LookAt, Vector4.Zero);
	}

	/// <summary>Swings at what is before it: its arm where it has one, or its whole body.</summary>
	public void Attack()
	{
		Set(NativeMobMotion.Attack, Vector4.Zero);
	}

	/// <summary>Flinches, as when it is hit.</summary>
	public void Hurt()
	{
		Set(NativeMobMotion.Hurt, Vector4.Zero);
	}

	/// <summary>
	/// Has it walk as if it moved at this velocity, in metres a second in the world, rather than by where its Obj
	/// went: for an Obj moved in jumps, as one a network places.
	/// </summary>
	public void MoveAs(Vector3 velocity)
	{
		Set(NativeMobMotion.Velocity, new Vector4(velocity, 1f));
	}

	/// <summary>Has it go by where its Obj moves again.</summary>
	public void FollowObj()
	{
		Set(NativeMobMotion.Velocity, Vector4.Zero);
	}

	/// <summary>Takes the Mob Motion off its Obj, whose model stands at rest.</summary>
	public void Remove()
	{
		NativeMobMotion.Set(Obj, NativeMobMotion.Remove, Vector4.Zero);
	}

	public static bool operator ==(MobMotion left, MobMotion right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(MobMotion left, MobMotion right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(MobMotion other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is MobMotion motion && Equals(motion);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Mob Motion";
	}

	private float Number(int what)
	{
		return NativeMobMotion.Number(Obj, what, 0f);
	}

	private void Set(int what, float value)
	{
		Set(what, new Vector4(value, 0f, 0f, 0f));
	}

	private void Set(int what, Vector4 value)
	{
		if (!NativeMobMotion.Set(Obj, what, value) && Native.IsConnected)
		{
			Log.Error($"{Obj} has no Mob Motion; give it one with AddMobMotion.");
		}
	}
}
