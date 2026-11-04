using System;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The level's seasons: where its Seasons component has the year, which turns its plants' leaves, drops them towards
/// winter and lays snow on them. Set a season outright, or move the time of year a little each frame to turn the year
/// as the game plays. Without a Seasons in the level, setting them does nothing and says so in the Console.
/// <code>
/// protected override void Update()
/// {
///     // A year every two minutes.
///     Seasons.TimeOfYear += Time.Delta * 4f / 120f;
/// }
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
}
