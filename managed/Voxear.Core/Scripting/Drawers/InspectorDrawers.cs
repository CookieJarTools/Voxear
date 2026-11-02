using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using System.Text;
using System.Text.Json;

namespace Voxear.Scripting;

/// <summary>
/// The drawers the Inspector draws the shown Obj's fields with, which live while it shows them, so a drawer can keep
/// what it likes between draws. With several Inspectors open, each one's drawers are kept apart, by the number the
/// editor gives it as "owner". When the Inspector is built, the editor opens a drawer for each the catalog names
/// on a field, and is given each one's markup to build; then, when something in a field's drawers was clicked or
/// changed, or its value changed elsewhere, it draws them with the value, innermost first, each with the document
/// its markup became. Requests and answers are JSON, through InspectorCalls:
/// { "action": "open", "entity", "fields": [{ "type", "path": ["items", 2], "drawers", "label" }] }, answered with
/// { "fields": [{ "markups" }] }, and { "action": "draw", "fields": [{ "field", "documents", "value" }] }, answered
/// with { "fields": [{ "value", "changed" }] }. A field that could not be opened or drawn is answered with { "error" }, which
/// is logged.
/// <para>
/// A behaviour's inspector draws it whole, as a field with no path. Its value is the table of fields the scene
/// sets, and its answer the table of those the inspector changed.
/// </para>
/// </summary>
internal static class InspectorDrawers
{
	private static readonly Dictionary<int, List<DrawnField?>> owners = new Dictionary<int, List<DrawnField?>>();

	/// <summary>Lets go of the drawers the Inspector asking opened before, and opens those asked for.</summary>
	public static void Open(Utf8JsonWriter json, JsonElement request)
	{
		var fields = Of(request);
		fields.Clear();
		var obj = request.TryGetProperty("entity", out var entity) ? new Obj(entity.GetUInt64()) : Obj.None;
		json.WriteStartArray("fields");
		foreach (var asked in request.GetProperty("fields").EnumerateArray())
		{
			json.WriteStartObject();
			try
			{
				var field = OpenOne(asked, obj);
				WriteMarkups(json, field);
				fields.Add(field);
			}
			catch (Exception exception)
			{
				fields.Add(null);
				Failed(json, exception);
			}
			json.WriteEndObject();
		}
		json.WriteEndArray();
	}

	/// <summary>Draws each field asked for with the drawers whose markup the editor built, innermost first.</summary>
	public static void Draw(Utf8JsonWriter json, JsonElement request)
	{
		var fields = Of(request);
		json.WriteStartArray("fields");
		foreach (var asked in request.GetProperty("fields").EnumerateArray())
		{
			json.WriteStartObject();
			try
			{
				DrawOne(json, asked, fields);
			}
			catch (Exception exception)
			{
				Failed(json, exception);
			}
			json.WriteEndObject();
		}
		json.WriteEndArray();
	}

	/// <summary>Lets go of every drawer, as when the scripts reload, so the old scripts can leave memory.</summary>
	public static void Forget()
	{
		owners.Clear();
	}

	/// <summary>The drawers of the Inspector a request is from, its "owner"; the first one's when it names none.</summary>
	private static List<DrawnField?> Of(JsonElement request)
	{
		var owner = request.TryGetProperty("owner", out var named) ? named.GetInt32() : 0;
		if (!owners.TryGetValue(owner, out var fields))
		{
			fields = new List<DrawnField?>();
			owners[owner] = fields;
		}
		return fields;
	}

	private static DrawnField OpenOne(JsonElement asked, Obj obj)
	{
		var (field, valueType) = Resolve(asked.GetProperty("type").GetString() ?? string.Empty, asked.GetProperty("path"));
		var label = asked.TryGetProperty("label", out var named) ? named.GetString() ?? string.Empty : string.Empty;
		var drawers = new List<Drawer>();
		foreach (var name in asked.GetProperty("drawers").EnumerateArray())
		{
			drawers.Add(Make(name.GetString() ?? string.Empty, field, label));
		}
		return new DrawnField(valueType, drawers, obj);
	}

	/// <summary>A drawer of the type named, given the field's attribute when it draws for one.</summary>
	private static Drawer Make(string name, FieldInfo? field, string label)
	{
		var type = FindType(name) ?? throw new ArgumentException($"There is no drawer {name}.");
		var drawer = (Drawer)Activator.CreateInstance(type, nonPublic: true)!;
		drawer.Label = label;
		if (DrawerTable.KindOf(type)?.Attribute is { } attributeType)
		{
			var attribute = field?.GetCustomAttribute(attributeType, inherit: true)
				?? throw new ArgumentException($"{field?.Name ?? label} is not marked with {attributeType.Name}, which {type.Name} draws.");
			drawer.Attach(attribute);
		}
		return drawer;
	}

	/// <summary>
	/// The field a path ends at, from the behaviour or component type it starts on, and the type of the value there:
	/// a name steps into a field, and a number into a list's item. No path is the behaviour itself, for its inspector.
	/// </summary>
	private static (FieldInfo? Field, Type Value) Resolve(string typeName, JsonElement path)
	{
		var current = FindType(typeName) ?? throw new ArgumentException($"There is no type {typeName}.");
		FieldInfo? field = null;
		foreach (var step in path.EnumerateArray())
		{
			if (step.ValueKind == JsonValueKind.Number)
			{
				current = DrawerTable.ItemType(current) ?? throw new ArgumentException($"{current.Name} is not a list.");
				continue;
			}
			var name = BehaviourFields.Normalize(step.GetString() ?? string.Empty);
			field = BehaviourFields.Settable(current).Find(f => BehaviourFields.Normalize(f.Name) == name)
				?? throw new ArgumentException($"{current.Name} has no field {step.GetString()}.");
			current = field.FieldType;
		}
		if (field == null && !typeof(Behaviour).IsAssignableFrom(current))
		{
			throw new ArgumentException($"{current.Name} is not a behaviour, which only an inspector draws whole.");
		}
		return (field, current);
	}

	private static void WriteMarkups(Utf8JsonWriter json, DrawnField field)
	{
		json.WriteStartArray("markups");
		foreach (var drawer in field.Drawers)
		{
			json.WriteStringValue(drawer.MarkupToBuild);
		}
		json.WriteEndArray();
	}

	private static void DrawOne(Utf8JsonWriter json, JsonElement asked, List<DrawnField?> fields)
	{
		var index = asked.GetProperty("field").GetInt32();
		var field = index >= 0 && index < fields.Count ? fields[index] : null;
		if (field == null)
		{
			throw new ArgumentException("The Inspector asked to draw a field it did not open.");
		}

		var given = asked.GetProperty("value");
		if (field.IsBehaviour)
		{
			DrawBehaviour(json, given, field, asked.GetProperty("documents"));
			return;
		}
		var value = given.ValueKind == JsonValueKind.Null ? null : FieldValues.Read(given, field.ValueType, field.Drawers[0].Label);
		var documents = new List<int>();
		foreach (var document in asked.GetProperty("documents").EnumerateArray())
		{
			documents.Add(document.GetInt32());
		}
		var before = Written(value, field.ValueType);
		for (var i = Math.Min(documents.Count, field.Drawers.Count) - 1; i >= 0; i--)
		{
			value = field.Drawers[i].Draw(value, new UiDocument(documents[i]));
		}
		var after = Written(value, field.ValueType);
		json.WritePropertyName("value");
		json.WriteRawValue(after);
		json.WriteBoolean("changed", after != before);
	}

	/// <summary>
	/// Draws a behaviour's inspector with a copy of the behaviour made from the fields the scene sets, on its Obj,
	/// and answers with the fields it changed, as a scene writes them.
	/// </summary>
	private static void DrawBehaviour(Utf8JsonWriter json, JsonElement given, DrawnField field, JsonElement documents)
	{
		var behaviour = (Behaviour)Activator.CreateInstance(field.ValueType, nonPublic: true)!;
		behaviour.Attach(field.Obj, 0, 0);
		if (given.ValueKind == JsonValueKind.Object)
		{
			BehaviourFields.Fill(behaviour, given, $"{field.ValueType.Name} on {field.Obj.Name}");
		}
		var before = InspectorCalls.Values(behaviour);
		foreach (var document in documents.EnumerateArray())
		{
			field.Drawers[0].Draw(behaviour, new UiDocument(document.GetInt32()));
			break;
		}
		var changed = false;
		json.WriteStartObject("value");
		foreach (var (name, value) in InspectorCalls.Values(behaviour))
		{
			if (before[name] != value)
			{
				json.WritePropertyName(name);
				json.WriteRawValue(value);
				changed = true;
			}
		}
		json.WriteEndObject();
		json.WriteBoolean("changed", changed);
	}

	/// <summary>A value as a scene writes it, to tell whether the drawers changed it.</summary>
	private static string Written(object? value, Type type)
	{
		var stream = new MemoryStream();
		using (var json = new Utf8JsonWriter(stream))
		{
			FieldDefaults.Write(json, value, type);
		}
		return Encoding.UTF8.GetString(stream.ToArray());
	}

	private static void Failed(Utf8JsonWriter json, Exception exception)
	{
		var inner = exception is TargetInvocationException { InnerException: { } thrown } ? thrown : exception;
		Log.Exception(inner);
		json.WriteString("error", inner.Message);
	}

	/// <summary>A type of the game scripts, or of what the engine was loaded with, as tests are.</summary>
	private static Type? FindType(string name)
	{
		return In(GameScripts.Assemblies, name)
			?? In(Array.FindAll(AppDomain.CurrentDomain.GetAssemblies(), assembly => !assembly.IsCollectible), name);
	}

	private static Type? In(IEnumerable<Assembly> assemblies, string name)
	{
		foreach (var assembly in assemblies)
		{
			if (assembly.GetType(name) is { } type)
			{
				return type;
			}
		}
		return null;
	}
}
