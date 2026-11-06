using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Buoyancy, which floats it on the sea, Pools and Streams: the water pushes it up by how far each part of
/// its Collider is under, and slows it against the water's own movement, so it rides the waves and a Stream carries
/// it along. Get it with Obj.Buoyancy, or give an Obj one with Obj.AddBuoyancy. It needs a Rigidbody and a Collider
/// beside it; how much it weighs does not change how it floats.
/// <code>
/// // A barrel that fills with water and sinks.
/// protected override void FixedUpdate()
/// {
///     var buoyancy = Obj.Buoyancy;
///     buoyancy.Submerged += 0.02f * Time.FixedDelta;
/// }
/// </code>
/// </summary>
public readonly struct Buoyancy : IEquatable<Buoyancy>, IObjComponent
{
	public static readonly Buoyancy None = default;

	internal Buoyancy(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj the water floats.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Buoyancy, or is gone.</summary>
	public bool IsValid => Obj.IsValid && NativePhysics.Flag(Obj, NativePhysics.BuoyancyHas);

	/// <summary>How much of it sits under still water, from 0 to 1: a boat's half, a log's more. Over 1, it sinks.</summary>
	public float Submerged
	{
		get { return NativePhysics.Number(Obj, NativePhysics.BuoyancySubmerged, 0.5f); }
		set { Set(NativePhysics.BuoyancySubmerged, value); }
	}

	/// <summary>
	/// How quickly the water slows it against the way the water moves: 1 stops it bobbing without its overshooting,
	/// less lets it bob longer.
	/// </summary>
	public float Drag
	{
		get { return NativePhysics.Number(Obj, NativePhysics.BuoyancyDrag, 0.4f); }
		set { Set(NativePhysics.BuoyancyDrag, value); }
	}

	/// <summary>How quickly the water stops it rocking and turning, as Drag does its moving.</summary>
	public float AngularDrag
	{
		get { return NativePhysics.Number(Obj, NativePhysics.BuoyancyAngularDrag, 0.4f); }
		set { Set(NativePhysics.BuoyancyAngularDrag, value); }
	}

	/// <summary>Takes the Buoyancy off its Obj, which then sinks like a stone.</summary>
	public void Remove()
	{
		NativePhysics.Set(Obj, NativePhysics.BuoyancyRemove, Vector4.Zero);
	}

	public static bool operator ==(Buoyancy left, Buoyancy right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(Buoyancy left, Buoyancy right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(Buoyancy other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is Buoyancy buoyancy && Equals(buoyancy);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Buoyancy";
	}

	private void Set(int what, float value)
	{
		if (!NativePhysics.Set(Obj, what, new Vector4(value, 0f, 0f, 0f)) && Native.IsConnected)
		{
			Log.Error($"{Obj} has no Buoyancy; give it one with AddBuoyancy.");
		}
	}
}
