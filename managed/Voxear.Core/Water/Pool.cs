using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Pool: water, or lava, at its Obj's height over a box of ground, turned with its Obj: a lake, a pond or a
/// flooded room. Get it with Obj.Pool, null when the Obj has none, or give an Obj one with Obj.AddPool.
/// <code>
/// // A room flooding: the water deepens a little each frame.
/// if (Obj.Pool is { } flood)
/// {
///     flood.Depth = MathF.Min(2f, flood.Depth + 0.1f * Time.Delta);
/// }
/// </code>
/// </summary>
public sealed class Pool : WholeComponent
{
	internal Pool(Obj obj)
		: base(obj, NativeWhole.Pool)
	{
	}

	/// <summary>Across and along, in metres.</summary>
	public Vector2 Size
	{
		get => Two("size", new Vector2(10f, 10f));
		set => Change("size", PostJson.Of(value));
	}

	/// <summary>How deep the water is, in metres: its walls and bottom show from outside.</summary>
	public float Depth
	{
		get => Number("depth", 4f);
		set => Change("depth", value);
	}

	/// <summary>How much of the sea's long swell it takes, from 0 for none to 1 as the sea's.</summary>
	public float Waves
	{
		get => Number("waves", 0f);
		set => Change("waves", value);
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
		get => Colour("deep", new Color(0.04f, 0.46f, 0.33f));
		set => Change("deep", PostJson.Of(value));
	}

	/// <summary>The colour light takes through its ripples.</summary>
	public Color Scatter
	{
		get => Colour("scatter", new Color(0.15f, 0.6f, 0.5f));
		set => Change("scatter", PostJson.Of(value));
	}

	/// <summary>What a metre of it lets through, by colour.</summary>
	public Color Clarity
	{
		get => Colour("clarity", new Color(0.7f, 0.97f, 0.93f));
		set => Change("clarity", PostJson.Of(value));
	}

	/// <summary>How murky it is: 1 as Clarity says, more to see less far into it.</summary>
	public float Murk
	{
		get => Number("murk", 1f);
		set => Change("murk", value);
	}

	/// <summary>Lava's: how many seconds its surface takes to crust over.</summary>
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

	/// <summary>The Obj's Pool, or null when it has none.</summary>
	internal static Pool? Of(Obj obj)
	{
		var found = new Pool(obj);
		return found.IsValid ? found : null;
	}

	public override string ToString()
	{
		return $"{Obj}'s Pool";
	}
}
