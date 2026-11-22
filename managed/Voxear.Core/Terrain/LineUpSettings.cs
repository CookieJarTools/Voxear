namespace Voxear;

/// <summary>
/// How Placing.LineUp lines prefabs up along a Road: which side, the stretch from From to To metres along it (To 0 for
/// its end), how far back from its edge each stands give or take Jitter, how far apart, and the seed: the same seed
/// gives the same street.
/// </summary>
public readonly record struct LineUpSettings(
	RoadSide Side = RoadSide.Both,
	float From = 0f,
	float To = 0f,
	float Setback = 1f,
	float Jitter = 0.5f,
	float Gap = 1f,
	ulong Seed = 1);
