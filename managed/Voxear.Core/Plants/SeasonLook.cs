namespace Voxear;

/// <summary>How plants look at the height of a season, as Seasons.Look reads it and Seasons.SetLook sets it.</summary>
/// <param name="Color">The colour leaves turn. The leaves' texture keeps its light and dark under it.</param>
/// <param name="ColorAmount">How far leaves turn to Color, from 0 for their own colour to 1.</param>
/// <param name="Snow">How much snow lies on plants, on what faces up, from 0 to 1.</param>
public readonly record struct SeasonLook(Color Color, float ColorAmount, float Snow);
