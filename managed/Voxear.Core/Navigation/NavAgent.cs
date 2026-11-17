using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Nav Agent: it finds its way to where it is sent and walks it, turning to face the way it goes, slowing
/// to stop, and going round other agents and characters on the way. The easy way is its Destination; the full way is
/// its Path, its velocities and how it moves. Get it with Obj.NavAgent, or give an Obj one with Obj.AddNavAgent.
/// <code>
/// // A guard that walks to wherever the bell rang.
/// Obj.NavAgent.Destination = bell.Position;
///
/// // And a villager that wanders the square until it is called.
/// if (Obj.NavAgent.Arrived)
/// {
/// 	Obj.NavAgent.Destination = Navigation.RandomPlace(square.Position, 12f);
/// }
/// </code>
/// </summary>
public readonly struct NavAgent : IEquatable<NavAgent>, IObjComponent
{
	private const int DestinationValue = 3;
	private const int StopValue = 4;
	private const int ResumeValue = 5;
	private const int StoppedValue = 6;
	private const int WarpValue = 7;
	private const int SpeedValue = 8;
	private const int AccelerationValue = 9;
	private const int TurnSpeedValue = 10;
	private const int StoppingDistanceValue = 11;
	private const int AutoBrakeValue = 12;
	private const int RadiusValue = 13;
	private const int HeightValue = 14;
	private const int AvoidanceValue = 15;
	private const int PriorityValue = 16;
	private const int MovesValue = 17;
	private const int SwimsValue = 18;
	private const int HasPathValue = 20;
	private const int PathPendingValue = 21;
	private const int ArrivedValue = 22;
	private const int StatusValue = 23;
	private const int RemainingValue = 24;
	private const int VelocityValue = 25;
	private const int WantedValue = 26;
	private const int NextCornerValue = 27;
	private const int CornerCountValue = 28;
	private const int CostValue = 29;
	private const int CornerAtValue = 30;
	private const int CornerValue = 31;
	private const int OpensDoorsValue = 33;
	private const int JumpsValue = 34;

	public static readonly NavAgent None = default;

	internal NavAgent(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj it walks.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Nav Agent, or is gone.</summary>
	public bool IsValid => Obj.IsValid && Number(NativeComponent.Has) != 0f;

	/// <summary>Where it is going, or null to stand. Set it and it finds its way there.</summary>
	public Vector3? Destination
	{
		get
		{
			var found = NativeComponent.Get(NativeComponent.NavAgent, Obj, DestinationValue, out var value);
			return found && value.W != 0f ? new Vector3(value.X, value.Y, value.Z) : null;
		}
		set { Write(DestinationValue, value is { } point ? new Vector4(point, 1f) : Vector4.Zero); }
	}

	/// <summary>Whether it is told to stand where it is, its destination kept for Resume.</summary>
	public bool IsStopped => Number(StoppedValue) != 0f;

	/// <summary>How fast it walks at most, in metres a second.</summary>
	public float Speed
	{
		get { return Number(SpeedValue); }
		set { Write(SpeedValue, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>How quickly it speeds up and slows down, in metres a second each second.</summary>
	public float Acceleration
	{
		get { return Number(AccelerationValue); }
		set { Write(AccelerationValue, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>How fast it turns to face the way it goes, in degrees a second.</summary>
	public float TurnSpeed
	{
		get { return Number(TurnSpeedValue); }
		set { Write(TurnSpeedValue, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>How near its destination it stops, in metres.</summary>
	public float StoppingDistance
	{
		get { return Number(StoppingDistanceValue); }
		set { Write(StoppingDistanceValue, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>Whether it slows down to stop at its destination, or arrives at speed.</summary>
	public bool AutoBrake
	{
		get { return Number(AutoBrakeValue) != 0f; }
		set { Write(AutoBrakeValue, new Vector4(value ? 1f : 0f, 0f, 0f, 0f)); }
	}

	/// <summary>How wide it is round its middle, in metres: how far others keep from it.</summary>
	public float Radius
	{
		get { return Number(RadiusValue); }
		set { Write(RadiusValue, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>How tall it is, in metres: what it fits under. With a Character Controller, the controller's height.</summary>
	public float Height
	{
		get { return Number(HeightValue); }
		set { Write(HeightValue, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>How carefully it goes round other agents and characters.</summary>
	public NavAvoidance Avoidance
	{
		get { return (NavAvoidance)(int)Number(AvoidanceValue); }
		set { Write(AvoidanceValue, new Vector4((int)value, 0f, 0f, 0f)); }
	}

	/// <summary>From 0 to 1: the higher, the more others go round it, and the less it goes round them.</summary>
	public float Priority
	{
		get { return Number(PriorityValue); }
		set { Write(PriorityValue, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>How it moves along its way.</summary>
	public NavMoves Moves
	{
		get { return (NavMoves)(int)Number(MovesValue); }
		set { Write(MovesValue, new Vector4((int)value, 0f, 0f, 0f)); }
	}

	/// <summary>Whether it swims across water too deep to stand in, and swims up in it, or goes round it and sinks.</summary>
	public bool Swims
	{
		get { return Number(SwimsValue) != 0f; }
		set { Write(SwimsValue, new Vector4(value ? 1f : 0f, 0f, 0f, 0f)); }
	}

	/// <summary>Whether it opens shut doors on its way and walks through, or goes round them.</summary>
	public bool OpensDoors
	{
		get { return Number(OpensDoorsValue) != 0f; }
		set { Write(OpensDoorsValue, new Vector4(value ? 1f : 0f, 0f, 0f, 0f)); }
	}

	/// <summary>Whether it jumps gaps one or two blocks across on its way, or goes round them.</summary>
	public bool Jumps
	{
		get { return Number(JumpsValue) != 0f; }
		set { Write(JumpsValue, new Vector4(value ? 1f : 0f, 0f, 0f, 0f)); }
	}

	/// <summary>Whether it has a way to walk.</summary>
	public bool HasPath => Number(HasPathValue) != 0f;

	/// <summary>Whether a way is being looked for: one asked for is there the next fixed step.</summary>
	public bool PathPending => Number(PathPendingValue) != 0f;

	/// <summary>Whether it is where it was sent, or has nowhere to go.</summary>
	public bool Arrived => Number(ArrivedValue, 1f) != 0f;

	/// <summary>Whether its way reaches its destination, or only as near as it can.</summary>
	public NavPathStatus PathStatus => (NavPathStatus)(int)Number(StatusValue);

	/// <summary>How far it has still to go along its way, in metres.</summary>
	public float RemainingDistance => Number(RemainingValue);

	/// <summary>How fast it goes, and which way, in metres a second.</summary>
	public Vector3 Velocity => Vector(VelocityValue);

	/// <summary>
	/// The velocity it wants, along its way and round others. With Moves set to NotAtAll, this is what a script or the
	/// motion matcher moves it by.
	/// </summary>
	public Vector3 DesiredVelocity => Vector(WantedValue);

	/// <summary>The corner of its way it is heading for.</summary>
	public Vector3 NextCorner => Vector(NextCornerValue);

	/// <summary>The way it is walking.</summary>
	public NavPath Path
	{
		get
		{
			var corners = new Vector3[(int)Number(CornerCountValue)];
			for (var at = 0; at < corners.Length; at++)
			{
				Write(CornerAtValue, new Vector4(at, 0f, 0f, 0f));
				corners[at] = Vector(CornerValue);
			}

			return new NavPath(PathStatus, corners, Number(CostValue));
		}
	}

	/// <summary>Sends it to <paramref name="point"/>.</summary>
	public void SetDestination(Vector3 point)
	{
		Destination = point;
	}

	/// <summary>Stands where it is, keeping its destination for Resume.</summary>
	public void Stop()
	{
		Write(StopValue, Vector4.Zero);
	}

	/// <summary>Goes on to its destination after Stop.</summary>
	public void Resume()
	{
		Write(ResumeValue, Vector4.Zero);
	}

	/// <summary>Moves it to <paramref name="point"/> at once, forgetting its way.</summary>
	public void Warp(Vector3 point)
	{
		Write(WarpValue, new Vector4(point, 0f));
	}

	/// <summary>Takes the Nav Agent off its Obj, which stays where it is.</summary>
	public void Remove()
	{
		NativeComponent.Set(NativeComponent.NavAgent, Obj, NativeComponent.Remove, Vector4.Zero);
	}

	public static bool operator ==(NavAgent left, NavAgent right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(NavAgent left, NavAgent right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(NavAgent other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is NavAgent agent && Equals(agent);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Nav Agent";
	}

	private float Number(int what, float otherwise = 0f)
	{
		return NativeComponent.Number(NativeComponent.NavAgent, Obj, what, otherwise);
	}

	private Vector3 Vector(int what)
	{
		return NativeComponent.Get(NativeComponent.NavAgent, Obj, what, out var value) ? new Vector3(value.X, value.Y, value.Z) : Vector3.Zero;
	}

	private void Write(int what, Vector4 value)
	{
		if (!NativeComponent.Set(NativeComponent.NavAgent, Obj, what, value) && Native.IsConnected)
		{
			Log.Error($"{Obj} has no Nav Agent; give it one with AddNavAgent.");
		}
	}
}
