namespace Voxear;

/// <summary>How a Cave Network lays out its rooms.</summary>
public enum CaveLayout
{
	/// <summary>Rooms in every direction without end.</summary>
	Endless,

	/// <summary>One bounded cave: a main path of rooms with side rooms off it.</summary>
	Level,
}
