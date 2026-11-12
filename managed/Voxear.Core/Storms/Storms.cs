using System;
using System.Collections.Generic;
using System.Numerics;

namespace Voxear;

/// <summary>
/// The level's storms: weather with a place and a size, that drifts, grows and dies. What falls from one is whatever
/// the temperature makes it where it blows: rain where it is mild, with thunder when it blows hard, snow and
/// blizzards where it is cold, blown sand where it is hot and dry.
/// <code>
/// // A storm over the harbour, at once.
/// var storm = Storms.Start(harbour.Position, 400f);
///
/// // Is the player out in it?
/// if (Storms.At(player.Transform.Position).Kind == StormWeather.Blizzard)
/// {
///     frost.Show();
/// }
/// </code>
/// With natural storms on (the level's Storms component, or <see cref="Natural"/>), storms also come and go by
/// themselves, the same for every player.
/// </summary>
public static class Storms
{
	private const int Arrive = 1;
	private const int Leave = 2;
	private const int Swallow = 3;

	/// <summary>Raised when a storm reaches the viewer.</summary>
	public static event Action<Storm>? Arrived;

	/// <summary>Raised when a storm leaves the viewer, or dies over them.</summary>
	public static event Action<Storm>? Left;

	/// <summary>
	/// Starts a storm over <paramref name="position"/> reaching <paramref name="radius"/> metres, blowing
	/// <paramref name="strength"/> hard, from 0 to 1. It comes on over a minute and blows until it is stopped;
	/// change what it gives back for anything else.
	/// </summary>
	public static Storm Start(Vector3 position, float radius = 400f, float strength = 1f, StormKind kind = StormKind.ByTemperature)
	{
		Span<float> numbers = stackalloc float[StormCalls.Numbers];
		numbers[0] = position.X;
		numbers[1] = position.Y;
		numbers[2] = position.Z;
		numbers[3] = radius;
		numbers[4] = strength;
		numbers[5] = (float)kind;
		var id = StormCalls.Call(StormCalls.Start, 0, numbers);
		return id > 0 ? new Storm((uint)id) : Storm.None;
	}

	/// <summary>The weather at <paramref name="position"/>: what falls there, how hard, and from which storm.</summary>
	public static WeatherAt At(Vector3 position)
	{
		Span<float> numbers = stackalloc float[StormCalls.Numbers];
		numbers[0] = position.X;
		numbers[1] = position.Y;
		numbers[2] = position.Z;
		var storm = StormCalls.Call(StormCalls.WeatherAt, 0, numbers);
		if (storm == -1 && !Interop.Native.IsConnected)
		{
			return default;
		}
		var amounts = new WeatherAmounts
		{
			Cloud = numbers[0],
			Darkness = numbers[1],
			Fog = numbers[2],
			Wind = numbers[3],
			Rain = numbers[4],
			Snow = numbers[5],
			Lightning = numbers[6],
			Dust = numbers[7],
		};
		return new WeatherAt(amounts, (StormWeather)(int)numbers[8], numbers[9], new Storm((uint)storm), numbers[10], numbers[11]);
	}

	/// <summary>Every storm near the viewer, natural and made, the nearest first, then every other made one.</summary>
	public static IReadOnlyList<Storm> All
	{
		get
		{
			var count = Math.Max(StormCalls.Call(StormCalls.Count, 0), 0);
			var found = new List<Storm>(count);
			for (var at = 0; at < count; at++)
			{
				var id = StormCalls.Call(StormCalls.IdAt, 0, at);
				if (id != 0)
				{
					found.Add(new Storm((uint)id));
				}
			}
			return found;
		}
	}

	/// <summary>Whether storms come and go by themselves.</summary>
	public static bool Natural
	{
		get => StormCalls.Rule(StormCalls.Natural) != 0f;
		set => StormCalls.SetRule(StormCalls.Natural, value ? 1f : 0f);
	}

	/// <summary>How often natural storms come, from 0 for never to 1.</summary>
	public static float Frequency
	{
		get => StormCalls.Rule(StormCalls.Frequency);
		set => StormCalls.SetRule(StormCalls.Frequency, value);
	}

	/// <summary>How far the largest natural storm reaches, in metres.</summary>
	public static float Largest
	{
		get => StormCalls.Rule(StormCalls.Largest);
		set => StormCalls.SetRule(StormCalls.Largest, value);
	}

	/// <summary>Whether great storms stand for ever at places of their own, each round a calm eye.</summary>
	public static bool Standing
	{
		get => StormCalls.Rule(StormCalls.Standing) != 0f;
		set => StormCalls.SetRule(StormCalls.Standing, value ? 1f : 0f);
	}

	/// <summary>
	/// The clock storms run on, in seconds, which turns while the game plays. Natural storms are worked out from it
	/// and the world's seed, so two machines with the same clock have the same storms: set a joining player's to the
	/// host's.
	/// </summary>
	public static double Clock
	{
		get
		{
			Span<float> numbers = stackalloc float[StormCalls.Numbers];
			return StormCalls.Call(StormCalls.Clock, 0, numbers) > 0 ? (double)numbers[0] + numbers[1] : 0d;
		}
		set
		{
			var whole = Math.Floor(value);
			StormCalls.Call(StormCalls.SetClock, 0, (float)whole, (float)(value - whole));
		}
	}

	/// <summary>Raises Arrived, Left and Sea.Swallowed for what has happened since last frame.</summary>
	internal static void Dispatch()
	{
		Span<float> numbers = stackalloc float[StormCalls.Numbers];
		for (var told = 0; told < 64; told++)
		{
			var kind = StormCalls.Call(StormCalls.Event, 0, numbers);
			if (kind <= 0)
			{
				return;
			}
			var about = BitConverter.SingleToUInt32Bits(numbers[0]) | ((ulong)BitConverter.SingleToUInt32Bits(numbers[1]) << 32);
			switch (kind)
			{
				case Arrive:
					Arrived?.Invoke(new Storm((uint)about));
					break;
				case Leave:
					Left?.Invoke(new Storm((uint)about));
					break;
				case Swallow:
					Sea.RaiseSwallowed(new Obj(about));
					break;
			}
		}
	}
}
