using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Nav Obstacle: a box Nav Agents and Block Mobs go round, though it was not there when the nav was made, as
/// a crate pushed into a doorway or a gate swung shut. Standing still it carves: it is cut out of the nav, so ways are
/// found round it, or stop short where it shuts the only one. Moving, it is gone round as agents go round each other,
/// and carves again once it has stood still for StillTime. Get it with Obj.NavObstacle, or give an Obj one with
/// Obj.AddNavObstacle.
/// <code>
/// // A portcullis that shuts the castle to everything finding its way in, and opens it again.
/// var gate = Obj.AddNavObstacle();
/// gate.Size = new Vector3(4f, 3f, 0.5f);
/// gate.Carve = shut;
/// </code>
/// </summary>
public readonly struct NavObstacle : IEquatable<NavObstacle>, IObjComponent
{
	private const int SizeValue = 3;
	private const int CarveValue = 4;
	private const int MoveThresholdValue = 5;
	private const int StillTimeValue = 6;
	private const int CarvingValue = 7;

	public static readonly NavObstacle None = default;

	internal NavObstacle(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj it stands in the way of.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Nav Obstacle, or is gone.</summary>
	public bool IsValid => Obj.IsValid && Number(NativeComponent.Has) != 0f;

	/// <summary>How wide, high and long its box is, in metres, centred on its Obj and turned and scaled with it.</summary>
	public Vector3 Size
	{
		get
		{
			var found = NativeComponent.Get(NativeComponent.NavObstacle, Obj, SizeValue, out var value);
			return found ? new Vector3(value.X, value.Y, value.Z) : Vector3.Zero;
		}
		set { Write(SizeValue, new Vector4(value, 0f)); }
	}

	/// <summary>Whether it is cut out of the nav where it stands still; off, it is only gone round.</summary>
	public bool Carve
	{
		get { return Number(CarveValue) != 0f; }
		set { Write(CarveValue, new Vector4(value ? 1f : 0f, 0f, 0f, 0f)); }
	}

	/// <summary>How far it moves, in metres, before it counts as moving and its cut is let go.</summary>
	public float MoveThreshold
	{
		get { return Number(MoveThresholdValue); }
		set { Write(MoveThresholdValue, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>How long it stands still, in seconds, before it carves where it stopped.</summary>
	public float StillTime
	{
		get { return Number(StillTimeValue); }
		set { Write(StillTimeValue, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>Whether it is cut out of the nav now: it carves, and has stood still long enough.</summary>
	public bool IsCarving => Number(CarvingValue) != 0f;

	/// <summary>Takes the Nav Obstacle off its Obj; ways are found through where it stood from the next step.</summary>
	public void Remove()
	{
		NativeComponent.Set(NativeComponent.NavObstacle, Obj, NativeComponent.Remove, Vector4.Zero);
	}

	public static bool operator ==(NavObstacle left, NavObstacle right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(NavObstacle left, NavObstacle right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(NavObstacle other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is NavObstacle obstacle && Equals(obstacle);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Nav Obstacle";
	}

	private float Number(int what)
	{
		return NativeComponent.Number(NativeComponent.NavObstacle, Obj, what, 0f);
	}

	private void Write(int what, Vector4 value)
	{
		if (!NativeComponent.Set(NativeComponent.NavObstacle, Obj, what, value) && Native.IsConnected)
		{
			Log.Error($"{Obj} has no Nav Obstacle; give it one with AddNavObstacle.");
		}
	}
}
