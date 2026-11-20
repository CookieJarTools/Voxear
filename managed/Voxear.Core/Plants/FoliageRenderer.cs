using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Foliage: one plant drawn wherever the Obj's plants grow. Its distances, its colours and its impostors
/// change as the game runs, as a graphics options menu does; its mesh and material are its scene's. Get it with
/// Obj.FoliageRenderer, null when the Obj has none. Foliage has the settings every plant shares.
/// <code>
/// // Grass drawn less far on a slow machine.
/// Obj.FoliageRenderer?.DrawDistance = 30f;
/// </code>
/// </summary>
public sealed class FoliageRenderer : WholeComponent
{
	internal FoliageRenderer(Obj obj)
		: base(obj, NativeWhole.Whole, NativeWhole.Foliage)
	{
	}

	/// <summary>How far from the camera plants are drawn, in metres.</summary>
	public float DrawDistance
	{
		get => Number("draw_distance", 60f);
		set => Change("draw_distance", value);
	}

	/// <summary>Over how many metres short of the draw distance they fade out.</summary>
	public float FadeDistance
	{
		get => Number("fade_distance", 10f);
		set => Change("fade_distance", value);
	}

	/// <summary>How far from the camera they cast shadows, in metres; 0 for none.</summary>
	public float ShadowDistance
	{
		get => Number("shadow_distance", 25f);
		set => Change("shadow_distance", value);
	}

	/// <summary>The colour plants are tinted towards in healthy patches; white leaves them as made.</summary>
	public Color HealthyColor
	{
		get => Colour("healthy_color", Color.White);
		set => Change("healthy_color", PostJson.Of(value));
	}

	/// <summary>The colour plants are tinted towards in dry patches; white leaves them as made.</summary>
	public Color DryColor
	{
		get => Colour("dry_color", Color.White);
		set => Change("dry_color", PostJson.Of(value));
	}

	/// <summary>How wide the patches that take each colour are across, in metres.</summary>
	public float ColorSpread
	{
		get => Number("color_spread", 8f);
		set => Change("color_spread", value);
	}

	/// <summary>When its far plants are drawn as impostors.</summary>
	public FoliageImpostors Impostors
	{
		get => Choice("impostors", FoliageImpostors.Auto);
		set => Change("impostors", PostJson.Of(value));
	}

	/// <summary>How far from the camera impostors start, in metres, when Impostors is From.</summary>
	public float ImpostorDistance
	{
		get => Number("impostor_distance", 60f);
		set => Change("impostor_distance", value);
	}

	internal static FoliageRenderer? Of(Obj obj)
	{
		var foliage = new FoliageRenderer(obj);
		return foliage.IsValid ? foliage : null;
	}

	public override string ToString()
	{
		return $"{Obj}'s Foliage";
	}
}
