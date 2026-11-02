using System;

namespace Voxear;

/// <summary>
/// Labels a field in the Inspector with words of its own instead of its name.
/// </summary>
[AttributeUsage(AttributeTargets.Field)]
public sealed class LabelTextAttribute : Attribute
{
	public LabelTextAttribute(string text)
	{
		Text = text;
	}

	public string Text { get; }
}
