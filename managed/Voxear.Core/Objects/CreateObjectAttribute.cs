using System;

namespace Voxear;

/// <summary>
/// Adds a prefab to the editor's Create Object, the Hierarchy's + menu and the palette's New …, under a path of
/// folders and its name. Put it on a static <see cref="Ref{T}"/> of a Prefab, in a game's scripts or a package's:
/// <code>
/// internal static class RifleObjects
/// {
/// 	[CreateObject("Guns/Rifle")]
/// 	private static readonly Ref&lt;Prefab&gt; Rifle = new("Prefabs/Rifle.prefab");
/// }
/// </code>
/// Picked, it makes the prefab's Objs where any new Obj goes, as Objs of the scene rather than an instance of the
/// prefab, as one step to undo. In a package's script, the prefab's path is from the package's folder.
/// <para>
/// The attribute takes the menu's path, and the field the prefab, since an attribute can hold only constants: a
/// Ref&lt;Prefab&gt; there would have to be a bare string.
/// </para>
/// </summary>
[AttributeUsage(AttributeTargets.Field | AttributeTargets.Property, Inherited = false)]
public sealed class CreateObjectAttribute : Attribute
{
	public CreateObjectAttribute(string path)
	{
		Path = path;
	}

	/// <summary>Its menus and its name, separated by slashes.</summary>
	public string Path { get; }
}
