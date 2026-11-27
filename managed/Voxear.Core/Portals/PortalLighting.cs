namespace Voxear;

/// <summary>How Portals' far sides light what stands in front of them (see Portal.Lighting).</summary>
public enum PortalLighting
{
	/// <summary>Not at all.</summary>
	Off,

	/// <summary>In one colour, the average of what each far side shows: a door onto a sunny field lights the room in daylight.</summary>
	Average,

	/// <summary>In what each far side shows, each part of the opening its own colour: blue from the sky, red from a fire beside it.</summary>
	Picture,
}
