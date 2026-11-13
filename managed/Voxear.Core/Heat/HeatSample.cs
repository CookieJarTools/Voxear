namespace Voxear;

/// <summary>
/// A place's temperature and what makes it so, as Heat.Sample gives it. Celsius is the sum of Climate, Day,
/// Sheltered, Zones and Sources.
/// </summary>
public readonly struct HeatSample
{
	internal HeatSample(float celsius, float climate, float day, float shelter, float sheltered, float zones, float sources, float humidity)
	{
		Celsius = celsius;
		Climate = climate;
		Day = day;
		Shelter = shelter;
		Sheltered = sheltered;
		Zones = zones;
		Sources = sources;
		Humidity = humidity;
	}

	/// <summary>The temperature there, in °C.</summary>
	public float Celsius { get; }

	/// <summary>The climate's temperature there, from the world's generator or the level's Climate.</summary>
	public float Climate { get; }

	/// <summary>What the time of day adds: most in the afternoon, least before dawn, more where it is dry.</summary>
	public float Day { get; }

	/// <summary>How sheltered it is by blocks overhead, from 0 in the open to 1 deep under ground.</summary>
	public float Shelter { get; }

	/// <summary>What shelter adds, drawing it to the ground's milder temperature, or what water does.</summary>
	public float Sheltered { get; }

	/// <summary>What Heat Zones add.</summary>
	public float Zones { get; }

	/// <summary>What hot and cold things near add: Heat Sources, blocks, lava and short-lived heat.</summary>
	public float Sources { get; }

	/// <summary>How wet the climate is there, from 0 to 1.</summary>
	public float Humidity { get; }

	public override string ToString()
	{
		return $"{Celsius:0.0} °C (climate {Climate:0.0}, day {Day:+0.0;-0.0}, shelter {Shelter:0.00}, sources {Sources:+0.0;-0.0})";
	}
}
