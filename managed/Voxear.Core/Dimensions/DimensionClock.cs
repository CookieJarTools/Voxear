using Voxear.Interop;

namespace Voxear;

/// <summary>
/// One dimension's clock: whose time of day it keeps, and that time. Got from <see cref="DimensionInstance.Clock"/>;
/// <see cref="Voxear.Clock"/> sets the level's.
/// </summary>
public readonly struct DimensionClock
{
	private readonly DimensionInstance dimension;

	internal DimensionClock(DimensionInstance dimension)
	{
		this.dimension = dimension;
	}

	/// <summary>Whose time it keeps: the level's, its own first Climate's, or none.</summary>
	public DimensionClockKind Keeps => (DimensionClockKind)(int)NativeDimensions.Clock(dimension.Handle).X;

	/// <summary>The hour there, from 0 to 24; null where it keeps no time, or has no Climate to keep it.</summary>
	public float? TimeOfDay => NativeDimensions.Clock(dimension.Handle).Y is var hour && float.IsNaN(hour) ? null : hour;

	/// <summary>How many minutes a whole day takes there; null where it keeps no time.</summary>
	public float? DayLength => NativeDimensions.Clock(dimension.Handle).Z is var minutes && float.IsNaN(minutes) ? null : minutes;

	/// <summary>Whether it is day there: from six to six. False where it keeps no time.</summary>
	public bool IsDay => TimeOfDay is >= 6f and < 18f;
}
