namespace Voxear;

/// <summary>How a Road meets the ground.</summary>
public enum RoadGround
{
	/// <summary>Laid on the ground as it is.</summary>
	Follow,

	/// <summary>The ground cut and filled to a line between its points, climbing no more than its Grade.</summary>
	Grade,
}
