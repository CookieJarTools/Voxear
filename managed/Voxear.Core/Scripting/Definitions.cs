using System;
using System.Collections.Generic;
using System.Text.Json;
using Voxear.Interop;

namespace Voxear.Scripting;

/// <summary>
/// The data assets scripts have loaded, by GUID and by every reference they were asked for by, so each file is one
/// instance. Cleared when scripts reload, as the instances are of the old build's classes.
/// </summary>
internal static class Definitions
{
	private static readonly Dictionary<string, Definition> loaded = new Dictionary<string, Definition>();

	public static Definition? Load(Type type, string? reference)
	{
		if (string.IsNullOrEmpty(reference))
		{
			return null;
		}
		if (!loaded.TryGetValue(reference, out var found))
		{
			found = Read(type, reference);
		}
		if (found == null || type.IsInstanceOfType(found))
		{
			return found;
		}
		Log.Error($"{found.Path} holds {found.GetType().Name}, not {type.Name}.");
		return null;
	}

	public static void Clear()
	{
		loaded.Clear();
	}

	private static Definition? Read(Type type, string reference)
	{
		var json = NativeData.Fields(reference);
		if (json == null)
		{
			Log.Error($"The project has no data asset \"{reference}\".");
			return null;
		}
		using var document = JsonDocument.Parse(json);
		var root = document.RootElement;
		var guid = root.GetProperty("guid").GetString() ?? reference;
		if (loaded.TryGetValue(guid, out var known))
		{
			loaded[reference] = known;
			return known;
		}
		var path = root.GetProperty("path").GetString() ?? reference;
		var made = Make(type, root.GetProperty("type").GetString(), path);
		if (made == null)
		{
			return null;
		}
		made.Name = System.IO.Path.GetFileNameWithoutExtension(path);
		made.Path = path;
		made.Guid = guid;
		BehaviourFields.Fill(made, root.GetProperty("fields"), path);
		loaded[guid] = made;
		loaded[reference] = made;
		return made;
	}

	/// <summary>An instance of the type the file names, found beside the type asked for, or of that type.</summary>
	private static Definition? Make(Type asked, string? named, string path)
	{
		var type = string.IsNullOrEmpty(named) || named == asked.FullName ? asked : asked.Assembly.GetType(named) ?? FindType(named);
		if (type == null || !typeof(Definition).IsAssignableFrom(type) || type.IsAbstract)
		{
			Log.Error($"{path} holds a {named}, which the scripts have no Definition for.");
			return null;
		}
		return (Definition)Activator.CreateInstance(type, nonPublic: true)!;
	}

	private static Type? FindType(string name)
	{
		foreach (var assembly in AppDomain.CurrentDomain.GetAssemblies())
		{
			if (assembly.GetType(name) is { } type)
			{
				return type;
			}
		}
		return null;
	}
}
