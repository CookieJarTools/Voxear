using System;

namespace Voxear
{
	/// <summary>
	/// Shows a field in the Inspector without letting it be edited there.
	/// </summary>
	[AttributeUsage(AttributeTargets.Field)]
	public sealed class ReadOnlyAttribute : Attribute
	{
	}
}
