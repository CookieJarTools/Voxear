using System;

namespace Voxear;

/// <summary>
/// Where Add Component lists a behaviour or a component, as Unity's attribute of the same name does: a path of
/// folders ending in the name it shows, <c>[AddComponentMenu("Effects/Camp Fire")]</c>. Without one, a script is
/// listed under Scripts, in folders for its namespace. An empty path leaves it out of Add Component altogether,
/// for a script that is only ever added from code.
/// </summary>
[AttributeUsage(AttributeTargets.Class | AttributeTargets.Struct, Inherited = false)]
public sealed class AddComponentMenuAttribute : Attribute
{
	public AddComponentMenuAttribute(string path)
	{
		Path = path;
	}

	/// <summary>The folders and name, separated by slashes, or empty to leave it out.</summary>
	public string Path { get; }
}
