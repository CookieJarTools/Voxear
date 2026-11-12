namespace Voxear;

/// <summary>
/// How much the weather adds of each thing it is made of, over the level's lighting as it is: each from 0 for none
/// to 1.
/// </summary>
public struct WeatherAmounts
{
	/// <summary>How far the sky's clouds are filled in, towards overcast.</summary>
	public float Cloud;

	/// <summary>How heavy and dark the clouds are, and how much of the sun they take.</summary>
	public float Darkness;

	/// <summary>How much fog and haze is added to the air.</summary>
	public float Fog;

	/// <summary>How much harder the level's Wind Zone blows.</summary>
	public float Wind;

	/// <summary>How hard it rains.</summary>
	public float Rain;

	/// <summary>How hard it snows.</summary>
	public float Snow;
}
