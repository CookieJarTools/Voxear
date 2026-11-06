using System;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using Voxear.Interop;
using Voxear.Scripting;

namespace Voxear.Saving;

/// <summary>
/// The C# components a save of the scene keeps: every one of a struct marked [Saved], on each Obj the save can find
/// again, as its whole value. Plain entities that are no Obj of a scene file or a prefab are left out, as there is no
/// finding them again. The types are only known at run time, so each is reached through a method made for it.
/// </summary>
internal static class SceneComponents
{
	private static readonly MethodInfo of = typeof(SceneComponents).GetMethod(nameof(Of), BindingFlags.NonPublic | BindingFlags.Static)!;
	private static readonly MethodInfo put = typeof(SceneComponents).GetMethod(nameof(Put), BindingFlags.NonPublic | BindingFlags.Static)!;

	/// <summary>Each kept component now, with the Obj it is on and its type.</summary>
	public static IEnumerable<SaveNode> Capture()
	{
		var types = GameScripts.Assemblies.SelectMany(Types).Where(Kept);
		return types.SelectMany(type => (List<SaveNode>)of.MakeGenericMethod(type).Invoke(null, null)!).ToList();
	}

	/// <summary>Gives a component back to its Obj, as it was saved.</summary>
	public static void Restore(SaveNode saved)
	{
		var typeName = saved["type"].AsString() ?? string.Empty;
		var key = SpawnedKeys.ForLookup(saved["obj"].AsString() ?? string.Empty);
		var obj = key == null ? Obj.None : NativeSaves.FindObj(key);
		var type = SaveTypes.Find(typeName);
		if (!obj.IsValid || type == null || !Kept(type))
		{
			Log.Warning($"A save of the scene has a {typeName} for an Obj or a component that is not here now, so it is left out.");
			return;
		}
		put.MakeGenericMethod(type).Invoke(null, new object[] { obj.Entity, saved["value"] });
	}

	private static bool Kept(Type type)
	{
		return SceneComponentTypes.IsComponent(type) && type.IsDefined(typeof(SavedAttribute), false);
	}

	private static IEnumerable<Type> Types(Assembly assembly)
	{
		try
		{
			return assembly.GetTypes();
		}
		catch (ReflectionTypeLoadException partly)
		{
			return partly.Types.OfType<Type>();
		}
	}

	private static List<SaveNode> Of<T>() where T : unmanaged, IComponent
	{
		var kept = new List<SaveNode>();
		foreach (var chunk in World.Query<T>())
		{
			Keep(kept, chunk.Entities, chunk.Read<T>());
		}
		return kept;
	}

	private static void Keep<T>(List<SaveNode> kept, ReadOnlySpan<Entity> entities, ReadOnlySpan<T> values)
	{
		for (var i = 0; i < entities.Length; i++)
		{
			var key = SpawnedKeys.ForSave(NativeSaves.Key(NativeSaves.KeyObj, entities[i].Bits, string.Empty));
			if (key != null)
			{
				kept.Add(Node(key, typeof(T), SaveNode.From(values[i])));
			}
		}
	}

	private static SaveNode Node(string key, Type type, SaveNode value)
	{
		var node = SaveNode.NewTable();
		node["obj"] = key;
		node["type"] = SaveTypes.NameOf(type);
		node["value"] = value;
		return node;
	}

	private static void Put<T>(Entity entity, SaveNode value) where T : unmanaged, IComponent
	{
		entity.Add(value.As<T>());
	}
}
