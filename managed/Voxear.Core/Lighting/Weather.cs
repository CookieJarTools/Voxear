using System;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The level's weather: cloud, gloom, fog, wind, rain, snow and lightning, laid over the level's lighting as it is.
/// Set a kind and how long it takes to come on; the sky fills, the light dims, the wind rises and the rain starts
/// together.
/// <code>
/// protected override void OnTriggerEnter(Collider other)
/// {
///     // A storm rolls in over half a minute.
///     Weather.Set(WeatherKind.Storm, 30f);
/// }
/// </code>
/// Lighting settings put the level in their own weather when they are applied, clear unless they say otherwise.
/// </summary>
public static unsafe class Weather
{
	private const int Numbers = 10;
	private const int Wet = 7;
	private const int Settled = 8;
	private const int Bright = 9;

	private static WeatherKind? lastKind;
	private static bool lit;

	/// <summary>Raised when the weather is set to another kind, by a script or by lighting settings.</summary>
	public static event Action<WeatherKind>? Changed;

	/// <summary>Raised as lightning strikes: the moment to play thunder.</summary>
	public static event Action? Struck;

	/// <summary>The kind of weather it is, or is turning to. Setting it turns the weather at once.</summary>
	public static WeatherKind Kind
	{
		get => (WeatherKind)Native.Api->WeatherState(null, null);
		set => Set(value, 0f);
	}

	/// <summary>What the weather adds this moment: part way between two kinds while it is turning.</summary>
	public static WeatherAmounts Amounts
	{
		get
		{
			var state = stackalloc float[Numbers];
			Native.Api->WeatherState(state, null);
			return new WeatherAmounts
			{
				Cloud = state[0],
				Darkness = state[1],
				Fog = state[2],
				Wind = state[3],
				Rain = state[4],
				Snow = state[5],
				Lightning = state[6],
			};
		}
	}

	/// <summary>Whether the weather is still turning to what it was last set to.</summary>
	public static bool IsTurning
	{
		get
		{
			var turning = 0;
			Native.Api->WeatherState(null, &turning);
			return turning != 0;
		}
	}

	/// <summary>
	/// How wet the rain has left the level, from 0 to 1. Rain soaks it in half a minute; it dries over a few once the
	/// rain stops.
	/// </summary>
	public static float Wetness => Read(Wet);

	/// <summary>How much snow has settled on the level, from 0 to 1. It melts once the snow stops, sooner in rain.</summary>
	public static float SnowCover => Read(Settled);

	/// <summary>How bright lightning is this moment, from 0 between strikes to 1.</summary>
	public static float Flash => Read(Bright);

	/// <summary>
	/// Turns the weather to a kind over <paramref name="seconds"/>, from whatever it is now, starting next frame.
	/// </summary>
	public static void Set(WeatherKind kind, float seconds = 10f)
	{
		Native.Api->WeatherSet((int)kind, null, seconds);
	}

	/// <summary>
	/// Turns the weather to amounts of its own over <paramref name="seconds"/>, each kept between 0 and 1: a light
	/// drizzle under a bright sky, fog without cloud.
	/// </summary>
	public static void Set(WeatherAmounts amounts, float seconds = 10f)
	{
		var given = stackalloc float[7]
		{
			amounts.Cloud, amounts.Darkness, amounts.Fog, amounts.Wind, amounts.Rain, amounts.Snow, amounts.Lightning,
		};
		Native.Api->WeatherSet((int)WeatherKind.Custom, given, seconds);
	}

	/// <summary>Raises Changed and Struck for what has happened since last frame.</summary>
	internal static void Dispatch()
	{
		var kind = Kind;
		if (lastKind != null && lastKind != kind)
		{
			Changed?.Invoke(kind);
		}
		lastKind = kind;
		var bright = Flash > 0f;
		if (bright && !lit)
		{
			Struck?.Invoke();
		}
		lit = bright;
	}

	private static float Read(int which)
	{
		var state = stackalloc float[Numbers];
		Native.Api->WeatherState(state, null);
		return state[which];
	}
}
