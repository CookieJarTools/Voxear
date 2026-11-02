using System;

namespace Voxear;

/// <summary>
/// What every Scene view handle on a field shares: drawn for the selected Obj in edit mode, dragged to change the
/// field, each drag a step to undo. Color sets the handle's colour, as hex such as "#ff8800".
/// </summary>
[AttributeUsage(AttributeTargets.Field)]
public abstract class HandleAttribute : Attribute
{
	private protected HandleAttribute()
	{
	}

	/// <summary>The handle's colour as hex, such as "#ff8800", or null for the usual.</summary>
	public string? Color { get; init; }
}
