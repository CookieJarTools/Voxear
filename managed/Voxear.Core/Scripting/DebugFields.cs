using System;
using System.Collections;
using System.Collections.Generic;
using System.Globalization;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Text;
using System.Text.Json;

namespace Voxear.Scripting;

/// <summary>
/// Every field of a behaviour as the Inspector's Debug mode shows it: public and private, saved with the scene or
/// not, its own and those of the classes it comes from, Behaviour's included. While the game plays it reads the
/// running instance; in edit mode, a copy made with the scene's values, as InspectorCalls makes one. Through
/// InspectorCalls: { "action": "debug-read", "behaviour", "entity", "fields", "handle"? }, answered with
/// { "running", "fields": [{ "name", "type", "declared", "access", "saved", "value" }] }.
/// </summary>
internal static class DebugFields
{
	private const BindingFlags Own =
		BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.DeclaredOnly;

	private const int MostItems = 8;

	public static void Read(Utf8JsonWriter json, JsonElement request)
	{
		var running = request.TryGetProperty("handle", out var handle) && handle.ValueKind == JsonValueKind.Number
			? BehaviourTable.Get(handle.GetUInt32())
			: null;
		object target = running ?? InspectorCalls.Make(request);
		var saved = new HashSet<FieldInfo>(BehaviourFields.Settable(target.GetType()));
		json.WriteBoolean("running", running != null);
		json.WriteStartArray("fields");
		for (var type = target.GetType(); type != null && type != typeof(object); type = type.BaseType)
		{
			foreach (var field in type.GetFields(Own))
			{
				json.WriteStartObject();
				json.WriteString("name", Named(field));
				json.WriteString("type", TypeName(field.FieldType));
				json.WriteString("declared", type.Name);
				json.WriteString("access", Access(field));
				json.WriteBoolean("saved", saved.Contains(field));
				json.WriteString("value", Describe(Read(field, target), 0));
				json.WriteEndObject();
			}
		}
		json.WriteEndArray();
	}

	private static object? Read(FieldInfo field, object target)
	{
		try
		{
			return field.GetValue(target);
		}
		catch (Exception exception)
		{
			return $"(could not be read: {exception.Message})";
		}
	}

	/// <summary>A field's name, an auto-property's backing field by its property's.</summary>
	private static string Named(FieldInfo field)
	{
		var name = field.Name;
		var generated = field.IsDefined(typeof(CompilerGeneratedAttribute)) && name.StartsWith('<');
		return generated && name.IndexOf('>') is > 1 and var end ? name[1..end] : name;
	}

	private static string Access(FieldInfo field) => field switch
	{
		{ IsPublic: true } => "public",
		{ IsFamily: true } => "protected",
		{ IsAssembly: true } => "internal",
		{ IsFamilyOrAssembly: true } => "protected internal",
		_ => "private",
	};

	private static string TypeName(Type type)
	{
		if (Nullable.GetUnderlyingType(type) is { } inner)
		{
			return $"{TypeName(inner)}?";
		}
		if (type.IsArray)
		{
			return $"{TypeName(type.GetElementType()!)}[]";
		}
		if (!type.IsGenericType)
		{
			return type.Name;
		}
		var name = type.Name[..type.Name.IndexOf('`')];
		var arguments = string.Join(", ", Array.ConvertAll(type.GetGenericArguments(), TypeName));
		return $"{name}<{arguments}>";
	}

	/// <summary>A value as words: a list as how many it holds and its first few, a float as it is kept.</summary>
	private static string Describe(object? value, int depth)
	{
		switch (value)
		{
			case null:
				return "null";
			case string text:
				return $"\"{text}\"";
			case float number:
				return number.ToString("R", CultureInfo.InvariantCulture);
			case double number:
				return number.ToString("R", CultureInfo.InvariantCulture);
			case IFormattable formattable:
				return formattable.ToString(null, CultureInfo.InvariantCulture);
			case IEnumerable items when depth < 2:
				return List(items, depth);
			default:
				return value.ToString() ?? value.GetType().Name;
		}
	}

	private static string List(IEnumerable items, int depth)
	{
		var listed = new StringBuilder();
		var count = 0;
		foreach (var item in items)
		{
			if (count < MostItems)
			{
				listed.Append(count == 0 ? "" : ", ").Append(Describe(item, depth + 1));
			}
			count++;
		}
		var more = count > MostItems ? ", …" : "";
		return $"[{count}] {listed}{more}";
	}
}
