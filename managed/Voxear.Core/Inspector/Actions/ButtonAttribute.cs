using System;

namespace Voxear;

/// <summary>
/// Shows a button under the behaviour's fields in the Inspector that calls this method, which takes nothing, in edit
/// mode as well as while playing. In edit mode it runs on a copy of the behaviour made from the scene's values, and
/// the fields it changes go back into the scene, as one step to undo.
/// </summary>
[AttributeUsage(AttributeTargets.Method)]
public sealed class ButtonAttribute : Attribute
{
	public ButtonAttribute(string? label = null)
	{
		Label = label;
	}

	/// <summary>What the button says, instead of the method's name in words.</summary>
	public string? Label { get; }
}
