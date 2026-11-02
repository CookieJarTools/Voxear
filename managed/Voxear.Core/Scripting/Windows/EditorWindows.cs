using System;
using System.Collections.Generic;
using System.Reflection;
using System.Text.Json;

namespace Voxear.Scripting;

/// <summary>
/// The game's editor windows the editor has open, each by the number it gave the window. Requests and answers are
/// JSON, through InspectorCalls: { "action": "window-open", "window", "type" }, answered with { "markup" };
/// { "action": "window-update", "windows": [{ "window", "document" }] }, answered with { "windows": [{}] }, or
/// { "error" } for one that threw, which is logged, or { "gone": true } for one let go of, as when the scripts
/// reload, for the editor to open again; and { "action": "window-close", "window" }.
/// </summary>
internal static class EditorWindows
{
	private static readonly Dictionary<int, EditorWindow> open = new Dictionary<int, EditorWindow>();

	public static void Open(Utf8JsonWriter json, JsonElement request)
	{
		var number = request.GetProperty("window").GetInt32();
		var name = request.GetProperty("type").GetString() ?? string.Empty;
		var type = Find(name) ?? throw new ArgumentException($"There is no window {name}.");
		var window = (EditorWindow)Activator.CreateInstance(type, nonPublic: true)!;
		CloseOne(number);
		open[number] = window;
		json.WriteString("markup", window.MarkupToBuild);
	}

	public static void Update(Utf8JsonWriter json, JsonElement request)
	{
		json.WriteStartArray("windows");
		foreach (var asked in request.GetProperty("windows").EnumerateArray())
		{
			json.WriteStartObject();
			UpdateOne(json, asked);
			json.WriteEndObject();
		}
		json.WriteEndArray();
	}

	public static void Close(JsonElement request)
	{
		CloseOne(request.GetProperty("window").GetInt32());
	}

	/// <summary>Lets go of every window, as when the scripts reload, so the old scripts can leave memory.</summary>
	public static void Forget()
	{
		foreach (var number in new List<int>(open.Keys))
		{
			CloseOne(number);
		}
	}

	/// <summary>Every window a scripts' assembly holds: not abstract or generic, and made with nothing.</summary>
	public static List<Type> In(IEnumerable<Type?> types)
	{
		var found = new List<Type>();
		foreach (var type in types)
		{
			if (type != null && Usable(type))
			{
				found.Add(type);
			}
		}
		return found;
	}

	private static bool Usable(Type type)
	{
		return typeof(EditorWindow).IsAssignableFrom(type) && !type.IsAbstract && !type.ContainsGenericParameters
			&& type.GetConstructor(BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic, Type.EmptyTypes) != null;
	}

	private static void UpdateOne(Utf8JsonWriter json, JsonElement asked)
	{
		if (!open.TryGetValue(asked.GetProperty("window").GetInt32(), out var window))
		{
			json.WriteBoolean("gone", true);
			return;
		}
		try
		{
			window.Run(new UiDocument(asked.GetProperty("document").GetInt32()));
		}
		catch (Exception exception)
		{
			var inner = exception is TargetInvocationException { InnerException: { } thrown } ? thrown : exception;
			Log.Exception(inner);
			json.WriteString("error", inner.Message);
		}
	}

	private static void CloseOne(int number)
	{
		if (!open.Remove(number, out var window))
		{
			return;
		}
		try
		{
			window.Close();
		}
		catch (Exception exception)
		{
			Log.Exception(exception);
		}
	}

	private static Type? Find(string name)
	{
		foreach (var assembly in GameScripts.Assemblies)
		{
			if (assembly.GetType(name) is { } type && Usable(type))
			{
				return type;
			}
		}
		return null;
	}
}
