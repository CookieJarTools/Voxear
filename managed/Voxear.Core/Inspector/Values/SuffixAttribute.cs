using System;

namespace Voxear
{
	/// <summary>
	/// Shows a unit after a field's value, such as "m/s" or "°".
	/// </summary>
	[AttributeUsage(AttributeTargets.Field)]
	public sealed class SuffixAttribute : Attribute
	{
		public SuffixAttribute(string suffix)
		{
			Suffix = suffix;
		}

		public string Suffix { get; }
	}
}
