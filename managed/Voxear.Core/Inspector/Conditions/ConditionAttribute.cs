using System;

namespace Voxear;

/// <summary>
/// A condition on another field of the same behaviour, named with nameof so renames carry through. With no value,
/// it holds while that field is true, non-zero, not empty or set; with one, while the field equals it. The editor
/// reads the field as the scene sets it, so conditions work without the game running.
/// </summary>
[AttributeUsage(AttributeTargets.Field)]
public abstract class ConditionAttribute : Attribute
{
	protected ConditionAttribute(string member, object? value)
	{
		Member = member;
		Value = value;
	}

	public string Member { get; }

	public object? Value { get; }
}
