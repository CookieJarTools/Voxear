using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The level's weather: cloud, gloom, fog, wind, rain and snow, laid over the level's lighting as it is. Set a kind
/// and how long it takes to come on; the sky fills, the light dims, the wind rises and the rain starts together.
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
			var amounts = stackalloc float[6];
			Native.Api->WeatherState(amounts, null);
			return new WeatherAmounts
			{
				Cloud = amounts[0],
				Darkness = amounts[1],
				Fog = amounts[2],
				Wind = amounts[3],
				Rain = amounts[4],
				Snow = amounts[5],
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
		var given = stackalloc float[6]
		{
			amounts.Cloud, amounts.Darkness, amounts.Fog, amounts.Wind, amounts.Rain, amounts.Snow,
		};
		Native.Api->WeatherSet((int)WeatherKind.Custom, given, seconds);
	}
}
