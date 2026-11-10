using System;
using System.Collections.Generic;
using System.Text.Json;
using Voxear.Interop;

namespace Voxear.Scripting;

/// <summary>
/// The data assets scripts have loaded, by GUID and by every reference they were asked for by, so each file is one
/// instance. Cleared when scripts reload, as the instances are of the old build's classes, and as a run of the game
/// ends, so values its code set on them go with it and the next run reads the files again.
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

	/// <summary>Every data asset of the project holding a <paramref name="type"/>, or a type derived from it, by path.</summary>
	public static List<Definition> All(Type type)
	{
		var found = new List<Definition>();
		var json = NativeData.All();
		if (json == null)
		{
			return found;
		}
		using var document = JsonDocument.Parse(json);
		foreach (var asset in document.RootElement.EnumerateArray())
		{
			var named = asset.GetProperty("type").GetString();
			var held = string.IsNullOrEmpty(named) ? null : type.Assembly.GetType(named) ?? FindType(named);
			if (held != null && type.IsAssignableFrom(held) && Load(type, asset.GetProperty("guid").GetString()) is { } loaded)
			{
				found.Add(loaded);
			}
		}
		return found;
	}

	public static void Clear()
	{
		loaded.Clear();
		// What was read through them: the next asking reads the files again.
		Items.Refresh();
		Recipes.Refresh();
		BlockInfos.Clear();
	}

	/// <summary>
	/// Changes the loaded instance of the data asset <paramref name="guid"/>, when there is one, as the Inspector changed
	/// its file: the fields in <paramref name="changed"/> are set, and those <paramref name="reset"/> names, which the
	/// file no longer gives, go back to the script's values. What holds the instance sees the new values at once, and
	/// its other fields keep what the game's code set on them.
	/// </summary>
	public static bool Refill(string guid, JsonElement changed, JsonElement reset)
	{
		if (!loaded.TryGetValue(guid, out var found))
		{
			return false;
		}
		if (reset.GetArrayLength() > 0)
		{
			Reset(found, reset);
		}
		BehaviourFields.Fill(found, changed, found.Path);
		return true;
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

	/// <summary>Puts each field named back to the value a new instance has, which is the one written in the script.</summary>
	private static void Reset(Definition found, JsonElement names)
	{
		var fresh = Activator.CreateInstance(found.GetType(), nonPublic: true)!;
		var fields = BehaviourFields.Settable(found.GetType());
		foreach (var name in names.EnumerateArray())
		{
			var key = BehaviourFields.Normalize(name.GetString() ?? string.Empty);
			var field = fields.Find(each => BehaviourFields.Normalize(each.Name) == key);
			field?.SetValue(found, field.GetValue(fresh));
		}
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
