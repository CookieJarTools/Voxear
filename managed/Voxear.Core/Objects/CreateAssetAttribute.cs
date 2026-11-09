using System;

namespace Voxear;

/// <summary>
/// Places a <see cref="Definition"/> in the list the Project panel's New Data Asset… offers, at a path of folders and
/// its name, as <c>[CreateAsset("Weapons/Weapon Stats")]</c>. A Definition without one is listed under Scripts, in a
/// folder for its namespace, and an empty path leaves it out.
/// </summary>
[AttributeUsage(AttributeTargets.Class, Inherited = false)]
public sealed class CreateAssetAttribute : Attribute
{
	public CreateAssetAttribute(string path)
	{
		Path = path;
	}

	/// <summary>Its menus and its name, separated by slashes.</summary>
	public string Path { get; }
}
