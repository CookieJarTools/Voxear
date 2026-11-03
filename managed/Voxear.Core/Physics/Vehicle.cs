using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Vehicle, which drives its Rigidbody on the Wheels of its child Objs with an engine, a gearbox and
/// differentials: a car, a tank on its tracks, or a motorbike. The Obj faces its +Z. Get it with Obj.Vehicle, or give
/// an Obj one with Obj.AddVehicle; it needs a Collider and a Rigidbody beside it.
/// <code>
/// var car = Obj.Vehicle;
/// car.Throttle = Input.Value("Accelerate");
/// car.Steering = Input.Vector2("Move").X;
/// car.Handbrake = Input.Held("Handbrake") ? 1f : 0f;
/// speedometer.Text = $"{car.Speed * 3.6f:0} km/h";
/// </code>
/// </summary>
public readonly struct Vehicle : IEquatable<Vehicle>, IObjComponent
{
	public static readonly Vehicle None = default;

	internal Vehicle(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj it drives.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Vehicle, or is gone.</summary>
	public bool IsValid => Obj.IsValid && NativePhysics.Flag(Obj, NativePhysics.VehicleHas);

	public VehicleKind Kind
	{
		get { return (VehicleKind)(int)NativePhysics.Number(Obj, NativePhysics.VehicleKind, 0f); }
		set { Set(NativePhysics.VehicleKind, (int)value); }
	}

	/// <summary>The most torque its engine gives, in newton-metres.</summary>
	public float MaxTorque
	{
		get { return NativePhysics.Number(Obj, NativePhysics.VehicleMaxTorque, 500f); }
		set { Set(NativePhysics.VehicleMaxTorque, value); }
	}

	/// <summary>The slowest its engine turns, in revolutions a minute, without stalling.</summary>
	public float MinRpm
	{
		get { return NativePhysics.Number(Obj, NativePhysics.VehicleMinRpm, 1000f); }
		set { Set(NativePhysics.VehicleMinRpm, value); }
	}

	/// <summary>The fastest its engine turns, in revolutions a minute.</summary>
	public float MaxRpm
	{
		get { return NativePhysics.Number(Obj, NativePhysics.VehicleMaxRpm, 6000f); }
		set { Set(NativePhysics.VehicleMaxRpm, value); }
	}

	/// <summary>Whether its gears change by themselves.</summary>
	public bool Automatic
	{
		get { return NativePhysics.Flag(Obj, NativePhysics.VehicleAutomatic); }
		set { Set(NativePhysics.VehicleAutomatic, value ? 1f : 0f); }
	}

	/// <summary>How many forward gears its gearbox has, each the same step down from the one before.</summary>
	public int Gears
	{
		get { return (int)NativePhysics.Number(Obj, NativePhysics.VehicleGears, 5f); }
		set { Set(NativePhysics.VehicleGears, value); }
	}

	/// <summary>First gear's ratio: higher pulls away harder but tops out sooner.</summary>
	public float FirstGear
	{
		get { return NativePhysics.Number(Obj, NativePhysics.VehicleFirstGear, 2.66f); }
		set { Set(NativePhysics.VehicleFirstGear, value); }
	}

	/// <summary>Top gear's ratio: lower reaches a higher top speed.</summary>
	public float TopGear
	{
		get { return NativePhysics.Number(Obj, NativePhysics.VehicleTopGear, 0.74f); }
		set { Set(NativePhysics.VehicleTopGear, value); }
	}

	/// <summary>
	/// How stiff the anti-roll bars across each pair of wheels side by side are, in newtons a metre, to keep it from
	/// leaning out of turns and rolling over: 0 for none, about 1000 for a car.
	/// </summary>
	public float AntiRoll
	{
		get { return NativePhysics.Number(Obj, NativePhysics.VehicleAntiRoll, 0f); }
		set { Set(NativePhysics.VehicleAntiRoll, value); }
	}

	/// <summary>How far down the accelerator is, from -1 for full reverse to 1 for full ahead.</summary>
	public float Throttle
	{
		get { return NativePhysics.Number(Obj, NativePhysics.VehicleThrottle, 0f); }
		set { Set(NativePhysics.VehicleThrottle, value); }
	}

	/// <summary>Which way it steers, from -1 for full left to 1 for full right.</summary>
	public float Steering
	{
		get { return NativePhysics.Number(Obj, NativePhysics.VehicleSteering, 0f); }
		set { Set(NativePhysics.VehicleSteering, value); }
	}

	/// <summary>How hard the brake is pressed, from 0 to 1.</summary>
	public float Brake
	{
		get { return NativePhysics.Number(Obj, NativePhysics.VehicleBrake, 0f); }
		set { Set(NativePhysics.VehicleBrake, value); }
	}

	/// <summary>How hard the hand brake is pulled, from 0 to 1. It holds the wheels with a Handbrake torque.</summary>
	public float Handbrake
	{
		get { return NativePhysics.Number(Obj, NativePhysics.VehicleHandbrake, 0f); }
		set { Set(NativePhysics.VehicleHandbrake, value); }
	}

	/// <summary>How fast it goes forward, in metres a second; below 0 in reverse. Times 3.6 is km/h.</summary>
	public float Speed => NativePhysics.Number(Obj, NativePhysics.VehicleSpeed, 0f);

	/// <summary>How fast its engine turns, in revolutions a minute.</summary>
	public float Rpm => NativePhysics.Number(Obj, NativePhysics.VehicleRpm, 0f);

	/// <summary>The gear it is in: -1 reverse, 0 neutral, 1 first and so on.</summary>
	public int Gear => (int)NativePhysics.Number(Obj, NativePhysics.VehicleGear, 0f);

	/// <summary>Takes the Vehicle off its Obj, which then rolls as an ordinary Rigidbody.</summary>
	public void Remove()
	{
		NativePhysics.Set(Obj, NativePhysics.VehicleRemove, Vector4.Zero);
	}

	public static bool operator ==(Vehicle left, Vehicle right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(Vehicle left, Vehicle right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(Vehicle other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is Vehicle vehicle && Equals(vehicle);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Vehicle";
	}

	private void Set(int what, float value)
	{
		if (!NativePhysics.Set(Obj, what, new Vector4(value, 0f, 0f, 0f)) && Native.IsConnected)
		{
			Log.Error($"{Obj} has no Vehicle; give it one with AddVehicle.");
		}
	}
}
