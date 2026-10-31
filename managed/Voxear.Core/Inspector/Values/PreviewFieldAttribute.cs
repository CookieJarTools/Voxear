using System;

namespace Voxear
{
	/// <summary>
	/// Shows a large preview of the asset a reference points at, under the field.
	/// </summary>
	[AttributeUsage(AttributeTargets.Field)]
	public sealed class PreviewFieldAttribute : Attribute
	{
		public PreviewFieldAttribute(int size = 96)
		{
			Size = size;
		}

		public int Size { get; }
	}
}
