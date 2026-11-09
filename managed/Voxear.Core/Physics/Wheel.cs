using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// One of a Vehicle's wheels, on a child Obj of the Vehicle's, placed where the wheel's middle is at rest, as Unity's
/// Wheel Collider is. While the game plays, physics turns and drops its Obj as the wheel rolls, steers and rides its
/// suspension, so a wheel model on the Obj, or inside it, turns with it. Get it with Obj.Wheel, or give an Obj one with
/// Obj.AddWheel.
/// </summary>
public readonly struct Wheel : IEquatable<Wheel>, IObjComponent
{
	public static readonly Wheel None = default;

	internal Wheel(Obj obj)
	{
		Obj = obj;
	}

	public Obj Obj { get; }

	/// <summary>False when its Obj has no Wheel, or is gone.</summary>
	public bool IsValid => Obj.IsValid && NativePhysics.Flag(Obj, NativePhysics.WheelHas);

	/// <summary>Whether it is switched on, as Unity's enabled. Off, its Vehicle drives without it.</summary>
	public bool Enabled
	{
		get { return Obj.IsEnabled(EngineComponent.Wheel); }
		set { Obj.SetEnabled(EngineComponent.Wheel, value); }
	}

	/// <summary>In metres.</summary>
	public float Radius
	{
		get { return NativePhysics.Number(Obj, NativePhysics.WheelRadius, 0.35f); }
		set { Set(NativePhysics.WheelRadius, value); }
	}

	public float Width
	{
		get { return NativePhysics.Number(Obj, NativePhysics.WheelWidth, 0.25f); }
		set { Set(NativePhysics.WheelWidth, value); }
	}

	/// <summary>How far its suspension travels, in metres, half up and half down from where it rests.</summary>
	public float Suspension
	{
		get { return NativePhysics.Number(Obj, NativePhysics.WheelSuspension, 0.3f); }
		set { Set(NativePhysics.WheelSuspension, value); }
	}

	/// <summary>How stiff its spring is, as how many times a second it bounces: 1 soft, 3 firm.</summary>
	public float Spring
	{
		get { return NativePhysics.Number(Obj, NativePhysics.WheelSpring, 1.5f); }
		set { Set(NativePhysics.WheelSpring, value); }
	}

	/// <summary>How quickly a bounce dies away, from 0 for not at all to 1 for at once.</summary>
	public float Damping
	{
		get { return NativePhysics.Number(Obj, NativePhysics.WheelDamping, 0.5f); }
		set { Set(NativePhysics.WheelDamping, value); }
	}

	/// <summary>How far it steers either way, in degrees: 0 for a wheel that does not.</summary>
	public float SteerAngle
	{
		get { return NativePhysics.Number(Obj, NativePhysics.WheelSteerAngle, 0f); }
		set { Set(NativePhysics.WheelSteerAngle, value); }
	}

	/// <summary>Whether the engine drives it.</summary>
	public bool Powered
	{
		get { return NativePhysics.Flag(Obj, NativePhysics.WheelPowered); }
		set { Set(NativePhysics.WheelPowered, value ? 1f : 0f); }
	}

	/// <summary>The most torque its brake holds it with, in newton-metres.</summary>
	public float BrakeTorque
	{
		get { return NativePhysics.Number(Obj, NativePhysics.WheelBrake, 1500f); }
		set { Set(NativePhysics.WheelBrake, value); }
	}

	/// <summary>The most torque the hand brake holds it with, in newton-metres: 0 for a wheel it does not hold.</summary>
	public float HandbrakeTorque
	{
		get { return NativePhysics.Number(Obj, NativePhysics.WheelHandbrake, 0f); }
		set { Set(NativePhysics.WheelHandbrake, value); }
	}

	/// <summary>
	/// How hard its tyre grips forwards, under the engine and the brakes, as a share of an ordinary tyre's: 2 holds
	/// twice as hard, as a racing tyre; 0.3 spins and slides, as on ice.
	/// </summary>
	public float Grip
	{
		get { return NativePhysics.Number(Obj, NativePhysics.WheelGrip, 1f); }
		set { Set(NativePhysics.WheelGrip, value); }
	}

	/// <summary>How hard its tyre grips sideways, through a turn, as a share of an ordinary tyre's.</summary>
	public float SideGrip
	{
		get { return NativePhysics.Number(Obj, NativePhysics.WheelSideGrip, 1f); }
		set { Set(NativePhysics.WheelSideGrip, value); }
	}

	public void Remove()
	{
		NativePhysics.Set(Obj, NativePhysics.WheelRemove, Vector4.Zero);
	}

	public static bool operator ==(Wheel left, Wheel right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(Wheel left, Wheel right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(Wheel other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is Wheel wheel && Equals(wheel);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Wheel";
	}

	private void Set(int what, float value)
	{
		if (!NativePhysics.Set(Obj, what, new Vector4(value, 0f, 0f, 0f)) && Native.IsConnected)
		{
			Log.Error($"{Obj} has no Wheel; give it one with AddWheel.");
		}
	}
}
