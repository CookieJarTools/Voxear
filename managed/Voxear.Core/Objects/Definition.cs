using System.Collections.Generic;
using System.Linq;
using Voxear.Scripting;

namespace Voxear;

/// <summary>
/// A data asset described in script, like a ScriptableObject: an <c>.asset</c> file holding a Definition's fields,
/// made from the Project panel's Create menu and edited in the Inspector. Every field that picks the same file shares
/// one instance of it, loaded the first time it is asked for.
/// <code>
/// [CreateAsset("Weapons/Weapon Stats")]
/// public sealed class WeaponStats : Definition
/// {
/// 	public float Damage = 10f;
/// 	public float Range = 50f;
/// }
///
/// public sealed class Rifle : Behaviour
/// {
/// 	public WeaponStats Stats;
/// }
/// </code>
/// </summary>
public abstract class Definition
{
	/// <summary>The file's name, without its folder or extension. One made in code has the name it is given.</summary>
	public string Name { get; set; } = string.Empty;

	/// <summary>The file's project path, as <c>Weapons/Rifle.asset</c>.</summary>
	public string Path { get; internal set; } = string.Empty;

	internal string Guid { get; set; } = string.Empty;

	/// <summary>
	/// A data asset by project path or GUID, the same instance every time; null, with an error logged, when the project
	/// has none, or it holds another type.
	/// </summary>
	public static T? Load<T>(string path) where T : Definition
	{
		return Definitions.Load(typeof(T), path) as T;
	}

	/// <summary>
	/// Every data asset of the project that holds a T, or a type derived from it, by path, each the instance Load
	/// gives: <c>Definition.All&lt;WeaponStats&gt;()</c>. It reads each data asset's file to find them, so ask once
	/// rather than every frame. It works in a running game, and in the editor's drawers, inspectors and windows.
	/// </summary>
	public static IReadOnlyList<T> All<T>() where T : Definition
	{
		return Definitions.All(typeof(T)).Cast<T>().ToList();
	}

	public override string ToString()
	{
		return string.IsNullOrEmpty(Path) ? GetType().Name : Path;
	}
}
