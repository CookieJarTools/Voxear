using System;

namespace Voxear;

/// <summary>
/// Calls a method of the behaviour, named with nameof and taking nothing, each time the field is changed in the
/// Inspector, as with a [Button]: the fields the method changes go back into the scene with the change.
/// </summary>
[AttributeUsage(AttributeTargets.Field)]
public sealed class OnValueChangedAttribute : Attribute
{
	public OnValueChangedAttribute(string method)
	{
		Method = method;
	}

	public string Method { get; }
}
