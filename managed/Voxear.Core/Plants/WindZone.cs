using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Wind Zone: wind blowing along its +Z, over the whole level or inside a ball or a box. Get it with
/// Obj.WindZone, null when the Obj has none, or give an Obj one with Obj.AddWindZone.
/// <code>
/// // A storm rising: the wind builds to a gale over a minute.
/// Obj.WindZone?.Strength = MathF.Min(3f, 1f + stormTime / 30f);
/// </code>
/// </summary>
public sealed class WindZone : WholeComponent
{
	internal WindZone(Obj obj)
		: base(obj, NativeWhole.WindZone)
	{
	}

	/// <summary>How hard it blows: 1 a breeze that sways trees, 3 a gale.</summary>
	public float Strength
	{
		get => Number("strength", 1f);
		set => Change("strength", value);
	}

	/// <summary>How much gusts add to it, from 0 for steady wind.</summary>
	public float Gusts
	{
		get => Number("gusts", 0.5f);
		set => Change("gusts", value);
	}

	/// <summary>How wide a gust is across, in metres.</summary>
	public float GustSize
	{
		get => Number("gust_size", 20f);
		set => Change("gust_size", value);
	}

	/// <summary>How fast gusts travel downwind, in metres a second.</summary>
	public float GustSpeed
	{
		get => Number("gust_speed", 6f);
		set => Change("gust_speed", value);
	}

	/// <summary>How much plants sway back and forth and across the wind, rather than lean steadily with it.</summary>
	public float Turbulence
	{
		get => Number("turbulence", 0.5f);
		set => Change("turbulence", value);
	}

	/// <summary>Where it blows: everywhere, or inside a ball or a box.</summary>
	public WindShape Shape
	{
		get => Choice("shape", WindShape.Global);
		set => Change("shape", PostJson.Of(value));
	}

	/// <summary>A Box's size, or a Sphere's width in X, in metres.</summary>
	public Vector3 Size
	{
		get => Three("size", new Vector3(60f, 20f, 60f));
		set => Change("size", PostJson.Of(value));
	}

	/// <summary>How far outside its shape its roughness fades into the sea's, in metres.</summary>
	public float Falloff
	{
		get => Number("falloff", 15f);
		set => Change("falloff", value);
	}

	/// <summary>The Obj's Wind Zone, or null when it has none.</summary>
	internal static WindZone? Of(Obj obj)
	{
		var zone = new WindZone(obj);
		return zone.IsValid ? zone : null;
	}

	public override string ToString()
	{
		return $"{Obj}'s Wind Zone";
	}
}
