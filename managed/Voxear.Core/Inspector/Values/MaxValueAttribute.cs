using System;

namespace Voxear;

/// <summary>
/// Keeps a number field at or below a value.
/// </summary>
[AttributeUsage(AttributeTargets.Field)]
public sealed class MaxValueAttribute : Attribute
{
	public MaxValueAttribute(double max)
	{
		Max = max;
	}

	public double Max { get; }
}
