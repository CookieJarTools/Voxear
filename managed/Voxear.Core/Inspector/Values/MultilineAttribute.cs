using System;

namespace Voxear
{
	/// <summary>
	/// Shows a text field over several lines.
	/// </summary>
	[AttributeUsage(AttributeTargets.Field)]
	public sealed class MultilineAttribute : Attribute
	{
		public MultilineAttribute(int lines = 3)
		{
			Lines = lines;
		}

		public int Lines { get; }
	}
}
