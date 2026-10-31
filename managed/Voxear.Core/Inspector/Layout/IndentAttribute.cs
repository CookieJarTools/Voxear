using System;

namespace Voxear
{
	/// <summary>
	/// Indents a field in the Inspector by a number of steps.
	/// </summary>
	[AttributeUsage(AttributeTargets.Field)]
	public sealed class IndentAttribute : Attribute
	{
		public IndentAttribute(int levels = 1)
		{
			Levels = levels;
		}

		public int Levels { get; }
	}
}
