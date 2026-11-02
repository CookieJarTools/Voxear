using System;

namespace Voxear;

/// <summary>
/// Text the Inspector shows when the pointer rests on a field's label.
/// </summary>
[AttributeUsage(AttributeTargets.Field)]
public sealed class TooltipAttribute : Attribute
{
	public TooltipAttribute(string text)
	{
		Text = text;
	}

	public string Text { get; }
}
