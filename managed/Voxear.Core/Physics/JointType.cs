namespace Voxear;

/// <summary>How a Joint holds its Obj.</summary>
public enum JointType
{
	/// <summary>Glued where they are when the game starts.</summary>
	Fixed,

	/// <summary>Turning about an axis through its anchor, as a door on its hinges.</summary>
	Hinge,

	/// <summary>Turning every way about its anchor, as a ball in its socket.</summary>
	Ball,

	/// <summary>Its centre kept within a range of its anchor, as a weight on a rope.</summary>
	Distance,
}
