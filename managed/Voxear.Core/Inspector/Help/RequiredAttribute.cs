using System;

namespace Voxear;

/// <summary>
/// Marks a reference that must be set: the Inspector shows an error on it while it is empty.
/// </summary>
[AttributeUsage(AttributeTargets.Field)]
public sealed class RequiredAttribute : Attribute
{
	public RequiredAttribute(string? message = null)
	{
		Message = message;
	}

	/// <summary>What the error says, instead of "{Field} is required".</summary>
	public string? Message { get; }
}
