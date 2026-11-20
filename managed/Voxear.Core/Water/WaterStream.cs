using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Stream: water, or lava, running along its Obj's Path, as wide as the Path is at each point and faster
/// where its bed falls more steeply. Get it with Obj.Stream, null when the Obj has none.
/// <code>
/// // A river in flood.
/// if (Obj.Stream is { } river)
/// {
///     river.Depth = 2.5f;
///     river.Murk = 4f;
/// }
/// </code>
/// </summary>
public sealed class WaterStream : WholeComponent
{
	internal WaterStream(Obj obj)
		: base(obj, NativeWhole.Stream)
	{
	}

	/// <summary>How deep it runs, in metres: deeper water runs faster downhill.</summary>
	public float Depth
	{
		get => Number("depth", 1f);
		set => Change("depth", value);
	}

	/// <summary>How fast it flows where its bed is level, in metres a second. Downhill it runs faster.</summary>
	public float Speed
	{
		get => Number("speed", 1f);
		set => Change("speed", value);
	}

	/// <summary>How steep it runs, as a slope, before it breaks white into rapids.</summary>
	public float Rapids
	{
		get => Number("rapids", 0.1f);
		set => Change("rapids", value);
	}

	/// <summary>Water, or lava.</summary>
	public Liquid Liquid
	{
		get => Choice("liquid", Liquid.Water);
		set => Change("liquid", PostJson.Of(value));
	}

	/// <summary>Its water's own colour, as an Ocean's Deep.</summary>
	public Color Deep
	{
		get => Colour("deep", new Color(0.05f, 0.36f, 0.3f));
		set => Change("deep", PostJson.Of(value));
	}

	/// <summary>The colour light takes through its ripples.</summary>
	public Color Scatter
	{
		get => Colour("scatter", new Color(0.2f, 0.55f, 0.45f));
		set => Change("scatter", PostJson.Of(value));
	}

	/// <summary>What a metre of it lets through, by colour.</summary>
	public Color Clarity
	{
		get => Colour("clarity", new Color(0.9f, 0.96f, 0.95f));
		set => Change("clarity", PostJson.Of(value));
	}

	/// <summary>How murky it is: 1 as Clarity says, more to see less far into it.</summary>
	public float Murk
	{
		get => Number("murk", 1f);
		set => Change("murk", value);
	}

	/// <summary>Lava's: how many seconds it runs before it takes to crust over.</summary>
	public float Cooling
	{
		get => Number("cooling", 20f);
		set => Change("cooling", value);
	}

	/// <summary>Lava's: how brightly it glows.</summary>
	public float Glow
	{
		get => Number("glow", 1f);
		set => Change("glow", value);
	}

	/// <summary>The Obj's Stream, or null when it has none.</summary>
	internal static WaterStream? Of(Obj obj)
	{
		var found = new WaterStream(obj);
		return found.IsValid ? found : null;
	}

	public override string ToString()
	{
		return $"{Obj}'s Stream";
	}
}
