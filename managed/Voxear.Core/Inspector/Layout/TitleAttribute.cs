using System;

namespace Voxear
{
	/// <summary>
	/// Shows a title, and a line under it, above a field in the Inspector, to start a part of its own.
	/// </summary>
	[AttributeUsage(AttributeTargets.Field)]
	public sealed class TitleAttribute : Attribute
	{
		public TitleAttribute(string title)
		{
			Title = title;
		}

		public string Title { get; }
	}
}
