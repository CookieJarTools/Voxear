using System;

namespace Voxear
{
	/// <summary>
	/// Keeps a number field at or above a value.
	/// </summary>
	[AttributeUsage(AttributeTargets.Field)]
	public sealed class MinValueAttribute : Attribute
	{
		public MinValueAttribute(double min)
		{
			Min = min;
		}

		public double Min { get; }
	}
}
