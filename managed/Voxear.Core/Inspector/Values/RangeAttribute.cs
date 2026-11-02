using System;

namespace Voxear;

/// <summary>
/// Limits a number field to a range, shown as a slider beside its value.
/// </summary>
[AttributeUsage(AttributeTargets.Field)]
public sealed class RangeAttribute : Attribute
{
	public RangeAttribute(double min, double max)
	{
		Min = min;
		Max = max;
	}

	public double Min { get; }

	public double Max { get; }
}
