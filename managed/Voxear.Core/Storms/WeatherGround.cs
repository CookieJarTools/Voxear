namespace Voxear;

/// <summary>
/// What the weather has left on the ground at a place, as <see cref="WeatherAt.Ground"/> gives it: each amount from 0
/// to 1, by place near the viewer and as the level's own weather leaves it further off. Under a roof, a tree or an
/// overhang, nothing is wet or snowed on.
/// </summary>
public readonly record struct WeatherGround(float Wet, float Snow, float Frozen, bool Sheltered)
{
	/// <summary>Whether snow lies there, enough to show.</summary>
	public bool Snowy => Snow > 0.3f;

	/// <summary>Whether the sea there is frozen hard enough to stand on.</summary>
	public bool Iced => Frozen > 0.9f;
}
