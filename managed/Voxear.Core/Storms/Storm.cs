using System;
using System.Numerics;

namespace Voxear;

/// <summary>
/// A storm: weather over a round part of the level that drifts, grows and dies. <see cref="Storms.Start"/> makes
/// one, and everything about it can be changed after. A storm the world made by itself can be read, not changed.
/// <code>
/// // A blizzard that crosses the valley and dies at the pass.
/// var storm = Storms.Start(valley.Position, 500f, 1f, StormKind.Blizzard);
/// storm.MoveAlong(6f, ford.Position, pass.Position);
/// storm.Lifetime = 600f;
/// </code>
/// </summary>
public readonly struct Storm : IEquatable<Storm>
{
	private const int RadiusNumber = 3;
	private const int StrengthNumber = 4;
	private const int KindNumber = 5;
	private const int EyeNumber = 8;
	private const int NowNumber = 9;
	private const int LeftNumber = 10;
	private const int NaturalNumber = 11;
	private const int ReachNumber = 12;

	/// <summary>No storm.</summary>
	public static readonly Storm None = default;

	private readonly uint id;

	internal Storm(uint id)
	{
		this.id = id;
	}

	/// <summary>Whether it is still there: false once it has died away, and for a natural one far from the viewer.</summary>
	public bool Exists => id != 0 && StormCalls.Call(StormCalls.Get, id) > 0;

	/// <summary>Whether the world made it by itself. Such a storm cannot be changed.</summary>
	public bool IsNatural => StormCalls.Read(StormCalls.Get, id, NaturalNumber) != 0f;

	/// <summary>Where its middle is, in the world. Its height is not used.</summary>
	public Vector3 Position
	{
		get
		{
			Span<float> numbers = stackalloc float[StormCalls.Numbers];
			return StormCalls.Call(StormCalls.Get, id, numbers) > 0 ? StormCalls.Place(numbers) : Vector3.Zero;
		}
		set => StormCalls.Call(StormCalls.SetPlace, id, value.X, value.Y, value.Z);
	}

	/// <summary>How far it reaches at its widest, in metres. It fades over the outer half.</summary>
	public float Radius
	{
		get => StormCalls.Read(StormCalls.Get, id, RadiusNumber);
		set => StormCalls.Call(StormCalls.SetRadius, id, value);
	}

	/// <summary>How far it reaches this moment: it spreads as it comes on.</summary>
	public float Reach => StormCalls.Read(StormCalls.Get, id, ReachNumber);

	/// <summary>How hard it blows at its height, from 0 to 1.</summary>
	public float Strength
	{
		get => StormCalls.Read(StormCalls.Get, id, StrengthNumber);
		set => StormCalls.Call(StormCalls.SetStrength, id, value);
	}

	/// <summary>How hard it blows at its middle this moment: less while it comes on and dies away.</summary>
	public float StrengthNow => StormCalls.Read(StormCalls.Get, id, NowNumber);

	/// <summary>What it is told to be. <see cref="StormKind.ByTemperature"/> unless set.</summary>
	public StormKind Kind
	{
		get => (StormKind)(int)StormCalls.Read(StormCalls.Get, id, KindNumber);
		set => StormCalls.Call(StormCalls.SetKind, id, (float)value);
	}

	/// <summary>How fast it drifts, in metres a second. Its height is not used.</summary>
	public Vector3 Velocity
	{
		get
		{
			Span<float> numbers = stackalloc float[StormCalls.Numbers];
			return StormCalls.Call(StormCalls.Get, id, numbers) > 0 ? new Vector3(numbers[6], 0f, numbers[7]) : Vector3.Zero;
		}
		set => StormCalls.Call(StormCalls.SetVelocity, id, value.X, value.Y, value.Z);
	}

	/// <summary>
	/// How much of its reach its calm eye takes, from 0 for none to 0.8: inside it the sky is clear and the sea
	/// flat, with the storm a wall all round.
	/// </summary>
	public float Eye
	{
		get => StormCalls.Read(StormCalls.Get, id, EyeNumber);
		set => StormCalls.Call(StormCalls.SetEye, id, value);
	}

	/// <summary>
	/// How many seconds it has left, dying away at the end of them; <see cref="float.PositiveInfinity"/> for one
	/// that blows until it is stopped.
	/// </summary>
	public float Lifetime
	{
		get
		{
			var left = StormCalls.Read(StormCalls.Get, id, LeftNumber);
			return left < 0f ? float.PositiveInfinity : left;
		}
		set => StormCalls.Call(StormCalls.SetLife, id, float.IsFinite(value) ? MathF.Max(value, 0f) : -1f);
	}

	/// <summary>How hard it blows at <paramref name="position"/> this moment, from 0 out of its reach to 1.</summary>
	public float StrengthAt(Vector3 position)
	{
		Span<float> numbers = stackalloc float[StormCalls.Numbers];
		numbers[0] = position.X;
		numbers[1] = position.Y;
		numbers[2] = position.Z;
		return StormCalls.Call(StormCalls.StrengthAt, id, numbers) > 0 ? numbers[0] : 0f;
	}

	/// <summary>
	/// Sends it to each of <paramref name="places"/> in turn at <paramref name="speed"/> metres a second, in place
	/// of any path it had. It stands at the last.
	/// </summary>
	public void MoveAlong(float speed, params Vector3[] places)
	{
		StormCalls.Call(StormCalls.PathClear, id);
		var way = Velocity;
		if (speed > 0f)
		{
			Velocity = way.LengthSquared() > 0f ? Vector3.Normalize(way) * speed : new Vector3(speed, 0f, 0f);
		}
		foreach (var place in places)
		{
			StormCalls.Call(StormCalls.PathAdd, id, place.X, place.Y, place.Z, speed);
		}
	}

	/// <summary>Has it die away over <paramref name="seconds"/>, and go.</summary>
	public void Stop(float seconds = 30f)
	{
		StormCalls.Call(StormCalls.Stop, id, seconds);
	}

	public static bool operator ==(Storm left, Storm right)
	{
		return left.id == right.id;
	}

	public static bool operator !=(Storm left, Storm right)
	{
		return left.id != right.id;
	}

	public bool Equals(Storm other)
	{
		return id == other.id;
	}

	public override bool Equals(object? obj)
	{
		return obj is Storm other && Equals(other);
	}

	public override int GetHashCode()
	{
		return (int)id;
	}
}
