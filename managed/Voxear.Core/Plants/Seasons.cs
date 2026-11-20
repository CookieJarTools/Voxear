using System;
using System.Text.Json.Nodes;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The level's seasons: where its Seasons component has the year, which turns its plants' leaves and lays
/// snow on them in winter. Set a season outright, or move the time of year a little each frame to turn the year
/// as the game plays, and change how each season looks. Without a Seasons in the level, setting them does nothing and
/// says so in the Console.
/// <code>
/// protected override void Update()
/// {
///     // A year every two minutes.
///     Seasons.TimeOfYear += Time.Delta * 4f / 120f;
/// }
///
/// // A blight: autumn comes early and brown, and the fields dry out.
/// Seasons.SetLook(Season.Autumn, Seasons.Look(Season.Autumn) with { Color = new Color(0.4f, 0.3f, 0.15f) });
/// Seasons.Dryness = 0.8f;
/// </code>
/// </summary>
public static unsafe class Seasons
{
	/// <summary>Whether the level has a Seasons to turn.</summary>
	public static bool Exists => !float.IsNaN(Native.Api->SeasonsTime());

	/// <summary>
	/// Where the year is, from 0 to 4: spring at 0, summer at 1, autumn at 2, winter at 3, and spring again at 4, which
	/// wraps round to 0. Between two seasons, plants are part way from one to the next.
	/// </summary>
	public static float TimeOfYear
	{
		get => Exists ? Native.Api->SeasonsTime() : 1f;
		set => Native.Api->SeasonsSetTime(value);
	}

	/// <summary>The season the year is nearest the height of; setting one turns the year to its height.</summary>
	public static Season Season
	{
		get => (Season)((int)MathF.Round(TimeOfYear) % 4);
		set => TimeOfYear = (int)value;
	}

	/// <summary>How plants look at the height of a season.</summary>
	public static SeasonLook Look(Season season)
	{
		var look = Level.Read()[Key(season)] as JsonObject ?? [];
		var color = new Color(0.3f, 0.5f, 0.15f);
		float amount = 0f, snow = 0f;
		PostJson.Read(look, "color", ref color);
		PostJson.Read(look, "color_amount", ref amount);
		PostJson.Read(look, "snow", ref snow);
		return new SeasonLook(color, amount, snow);
	}

	/// <summary>Sets how plants look at the height of a season.</summary>
	public static void SetLook(Season season, SeasonLook look)
	{
		Level.Change(Key(season), new JsonObject
		{
			["color"] = PostJson.Of(look.Color),
			["color_amount"] = look.ColorAmount,
			["snow"] = look.Snow,
		});
	}

	/// <summary>How unevenly plants follow the year, from 0 for all together to 1 for up to half a season early or late.</summary>
	public static float Variation
	{
		get => Level.Number("variation", 0.5f);
		set => Level.Change("variation", value);
	}

	/// <summary>How wide the patches that turn together, and dry together, are across, in metres.</summary>
	public static float PatchSize
	{
		get => Level.Number("patch_size", 25f);
		set => Level.Change("patch_size", value);
	}

	/// <summary>How much of a field dries out in patches, from 0 to 1.</summary>
	public static float Dryness
	{
		get => Level.Number("dryness", 0f);
		set => Level.Change("dryness", value);
	}

	/// <summary>The colour dry leaves turn.</summary>
	public static Color DryColor
	{
		get => Level.Colour("dry_color", new Color(0.62f, 0.52f, 0.25f));
		set => Level.Change("dry_color", PostJson.Of(value));
	}

	/// <summary>How wet plants are, from 0 to 1: darker, and shining.</summary>
	public static float Wetness
	{
		get => Level.Number("wetness", 0f);
		set => Level.Change("wetness", value);
	}

	/// <summary>How much of the snow and wetness also lie on the ground, rocks and roofs, from 0 for plants alone to 1.</summary>
	public static float GroundCover
	{
		get => Level.Number("ground_cover", 0f);
		set => Level.Change("ground_cover", value);
	}

	private static LevelSettings Level { get; } = new("seasons", "Seasons");

	private static string Key(Season season)
	{
		return PostJson.Key(season.ToString());
	}
}
