using System;

namespace Voxear;

/// <summary>
/// Lists a <see cref="Definition"/> in the Project panel's Create menu, at a path of menus and its name, as
/// <c>[CreateAsset("Weapons/Weapon Stats")]</c>. A Definition without one is made by copying another's file.
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
