using System;

namespace Voxear;

/// <summary>
/// Shows a message in a box above a field in the Inspector, and with visibleIf, only while that field of the same
/// behaviour is true, non-zero, not empty or set.
/// </summary>
[AttributeUsage(AttributeTargets.Field, AllowMultiple = true)]
public sealed class InfoBoxAttribute : Attribute
{
	public InfoBoxAttribute(string message, InfoMessageType type = InfoMessageType.Info, string? visibleIf = null)
	{
		Message = message;
		Type = type;
		VisibleIf = visibleIf;
	}

	public string Message { get; }

	public InfoMessageType Type { get; }

	public string? VisibleIf { get; }
}
