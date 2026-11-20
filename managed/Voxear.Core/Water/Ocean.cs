using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Ocean: the sea, filling the level at its Obj's height, its wind blowing along its +Z or the level's Wind
/// Zone's. The first one in the level is used. Its costlier looks are switched with Water.Features. Get it with
/// Obj.Ocean, null when the Obj has none.
/// <code>
/// // The sea rises as a storm comes in, and darkens.
/// if (Obj.Ocean is { } sea)
/// {
///     sea.Weather = SeaWeather.Stormy;
///     sea.Deep = new Color(0.02f, 0.05f, 0.07f);
/// }
/// </code>
/// </summary>
public sealed class Ocean : WholeComponent
{
	internal Ocean(Obj obj)
		: base(obj, NativeWhole.Ocean)
	{
	}

	/// <summary>The sea's state: calm, normal or stormy, or Custom for the swell and wind below.</summary>
	public SeaWeather Weather
	{
		get => Choice("weather", SeaWeather.Normal);
		set => Change("weather", PostJson.Of(value));
	}

	/// <summary>For Custom weather: the wind that raised the swell far off, in metres a second.</summary>
	public float SwellWind
	{
		get => Number("swell_wind", 9f);
		set => Change("swell_wind", value);
	}

	/// <summary>For Custom weather: how far that wind blew over open water, in kilometres.</summary>
	public float SwellFetch
	{
		get => Number("swell_fetch", 150f);
		set => Change("swell_fetch", value);
	}

	/// <summary>For Custom weather: the wind here, in metres a second.</summary>
	public float Wind
	{
		get => Number("wind", 7f);
		set => Change("wind", value);
	}

	/// <summary>For Custom weather: how far the wind here has blown over open water, in kilometres.</summary>
	public float Fetch
	{
		get => Number("fetch", 20f);
		set => Change("fetch", value);
	}

	/// <summary>How sharp the crests are: 0 for round waves, 1 as real ones.</summary>
	public float Choppiness
	{
		get => Number("choppiness", 0.9f);
		set => Change("choppiness", value);
	}

	/// <summary>How much higher the waves are than real ones: 1 as the weather makes them.</summary>
	public float WaveScale
	{
		get => Number("wave_scale", 1f);
		set => Change("wave_scale", value);
	}

	/// <summary>The water's own colour: what it fades into far off.</summary>
	public Color Deep
	{
		get => Colour("deep", new Color(0f, 0.3f, 0.34f));
		set => Change("deep", PostJson.Of(value));
	}

	/// <summary>The colour light takes through the thin tops of the waves.</summary>
	public Color Scatter
	{
		get => Colour("scatter", new Color(0.1f, 0.78f, 0.72f));
		set => Change("scatter", PostJson.Of(value));
	}

	/// <summary>How much foam the breaking crests have, from 0 for none.</summary>
	public float Foam
	{
		get => Number("foam", 0.5f);
		set => Change("foam", value);
	}

	/// <summary>How much spray flies off the breaking crests and the surf, from 0 for none.</summary>
	public float Spray
	{
		get => Number("spray", 0.5f);
		set => Change("spray", value);
	}

	/// <summary>What a metre of water lets through, by colour.</summary>
	public Color Clarity
	{
		get => Colour("clarity", new Color(0.82f, 0.97f, 0.98f));
		set => Change("clarity", PostJson.Of(value));
	}

	/// <summary>How murky it is: 1 as Clarity says, more to see less far into it.</summary>
	public float Murk
	{
		get => Number("murk", 1f);
		set => Change("murk", value);
	}

	/// <summary>Where its wind comes from: its own, or the level's Wind Zone.</summary>
	public OceanWind WindSource
	{
		get => Choice("wind_source", OceanWind.Own);
		set => Change("wind_source", PostJson.Of(value));
	}

	/// <summary>How many seconds its waves take to follow a change of wind, rather than jumping.</summary>
	public float Transition
	{
		get => Number("transition", 20f);
		set => Change("transition", value);
	}

	/// <summary>The Obj's Ocean, or null when it has none.</summary>
	internal static Ocean? Of(Obj obj)
	{
		var found = new Ocean(obj);
		return found.IsValid ? found : null;
	}

	public override string ToString()
	{
		return $"{Obj}'s Ocean";
	}
}
