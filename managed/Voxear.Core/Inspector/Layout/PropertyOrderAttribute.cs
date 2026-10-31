using System;

namespace Voxear
{
	/// <summary>
	/// Moves a field in the Inspector: lower orders come first, and fields without one are 0, in the order declared.
	/// </summary>
	[AttributeUsage(AttributeTargets.Field)]
	public sealed class PropertyOrderAttribute : Attribute
	{
		public PropertyOrderAttribute(int order)
		{
			Order = order;
		}

		public int Order { get; }
	}
}
