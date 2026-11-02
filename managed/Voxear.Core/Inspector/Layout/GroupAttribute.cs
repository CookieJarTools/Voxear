using System;

namespace Voxear;

/// <summary>
/// Puts a field in a group in the Inspector: the fields of a behaviour that name the same group show together, in
/// the order the first of them comes. A path such as "Movement/Air" puts a group inside another.
/// </summary>
[AttributeUsage(AttributeTargets.Field)]
public abstract class GroupAttribute : Attribute
{
	protected GroupAttribute(string name)
	{
		Name = name;
	}

	public string Name { get; }
}
