using System;

namespace Voxear;

/// <summary>
/// Shows a number field as a bar filled from min to max, which a drag sets.
/// </summary>
[AttributeUsage(AttributeTargets.Field)]
public sealed class ProgressBarAttribute : Attribute
{
	public ProgressBarAttribute(double min = 0, double max = 100)
	{
		Min = min;
		Max = max;
	}

	public double Min { get; }

	public double Max { get; }
}
