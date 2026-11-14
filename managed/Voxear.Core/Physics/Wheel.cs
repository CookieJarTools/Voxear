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

	/// <summary>An ordinary tyre's friction forwards against its slip ratio: most at 0.06, a little less once it slides.</summary>
	public static readonly Curve DefaultFriction = new((0f, 0f), (0.06f, 1.2f), (0.2f, 1f));

	/// <summary>And sideways against its slip angle, 0 to 90 degrees as 0 to 1: most at 3 degrees, a little less from 20.</summary>
	public static readonly Curve DefaultSideFriction = new((0f, 0f), (3f / 90f, 1.2f), (20f / 90f, 1f));

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

	/// <summary>
	/// How hard its tyre holds forwards, against its slip ratio: 0 rolling with the ground, 1 locked or spinning in
	/// place. DefaultFriction to start with; Grip scales it.
	/// <code>
	/// // A tyre that holds on, then lets go all at once.
	/// wheel.Friction = new Curve((0f, 0f), (0.1f, 1.4f), (0.15f, 0.6f));
	/// </code>
	/// </summary>
	public Curve Friction
	{
		get { return ReadCurve(NativePhysics.FrictionCurve, DefaultFriction); }
		set { Tell(NativePhysics.FrictionCurve, value); }
	}

	/// <summary>How hard its tyre holds sideways, against its slip angle from 0 to 90 degrees as 0 to 1; SideGrip scales it.</summary>
	public Curve SideFriction
	{
		get { return ReadCurve(NativePhysics.SideFrictionCurve, DefaultSideFriction); }
		set { Tell(NativePhysics.SideFrictionCurve, value); }
	}

	/// <summary>Whether its tyre touches the ground, after the last physics step.</summary>
	public bool IsGrounded => NativePhysics.Flag(Obj, NativePhysics.WheelTouching);

	/// <summary>Where its tyre touches the ground, in the world, while IsGrounded: where a skid mark goes.</summary>
	public Vector3 ContactPoint => NativePhysics.Vector(Obj, NativePhysics.WheelContactPoint, Vector3.Zero);

	/// <summary>Which way the ground faces where its tyre touches it.</summary>
	public Vector3 ContactNormal => NativePhysics.Vector(Obj, NativePhysics.WheelContactNormal, Vector3.UnitY);

	/// <summary>How far its suspension is pressed in, from 0 hanging to 1 against its stop.</summary>
	public float Compression => NativePhysics.Number(Obj, NativePhysics.WheelCompression, 0f);

	/// <summary>How much its tyre slips forwards: 0 rolling with the ground, 1 locked or spinning in place.</summary>
	public float Slip => NativePhysics.Number(Obj, NativePhysics.WheelSlip, 0f);

	/// <summary>How far its tyre slides sideways, as the angle between where it rolls and where it goes, in degrees.</summary>
	public float SlipAngle => NativePhysics.Number(Obj, NativePhysics.WheelSlipAngle, 0f);

	/// <summary>How fast it spins, in radians a second.</summary>
	public float Spin => NativePhysics.Number(Obj, NativePhysics.WheelSpin, 0f);

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

	private Curve ReadCurve(int which, Curve missing)
	{
		return Native.IsConnected ? Curve.FromWords(NativeComponent.Text(NativeComponent.Vehicle, Obj, which)) : missing;
	}

	private void Tell(int which, Curve curve)
	{
		if (!NativeComponent.Tell(NativeComponent.Vehicle, Obj, which + NativePhysics.CurveChange, curve.ToWords()) && Native.IsConnected)
		{
			Log.Error($"{Obj} has no Wheel; give it one with AddWheel.");
		}
	}

	private void Set(int what, float value)
	{
		if (!NativePhysics.Set(Obj, what, new Vector4(value, 0f, 0f, 0f)) && Native.IsConnected)
		{
			Log.Error($"{Obj} has no Wheel; give it one with AddWheel.");
		}
	}
}
