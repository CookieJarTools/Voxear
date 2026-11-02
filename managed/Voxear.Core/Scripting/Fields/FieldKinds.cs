using System;
using System.Collections.Generic;
using System.Numerics;
using System.Text.Json;

namespace Voxear.Scripting;

/// <summary>
/// Writes what kind of value a field takes, for the editor to pick a control, in the same order of checks
/// FieldValues reads values in: { "kind": "number" }, { "kind": "list", "element": ... } and so on.
/// [Serializable] types are written by name and noted in types, to be described once.
/// </summary>
internal static class FieldKinds
{
	public static void Write(Utf8JsonWriter json, Type type, Dictionary<string, Type> types)
	{
		json.WriteStartObject();
		if (!WritePlain(json, type) && !WriteStructured(json, type, types))
		{
			json.WriteString("kind", "unsupported");
			json.WriteString("name", type.FullName ?? type.Name);
		}
		json.WriteEndObject();
	}

	/// <summary>Text, toggles, choices and numbers.</summary>
	private static bool WritePlain(Utf8JsonWriter json, Type type)
	{
		if (type.IsEnum)
		{
			json.WriteString("kind", "enum");
			json.WriteStartArray("names");
			Array.ForEach(Enum.GetNames(type), json.WriteStringValue);
			json.WriteEndArray();
			return true;
		}

		var kind = Plain(type);
		if (kind != null)
		{
			json.WriteString("kind", kind);
		}
		return kind != null;
	}

	private static string? Plain(Type type)
	{
		if (type == typeof(string))
		{
			return "text";
		}
		if (type == typeof(bool))
		{
			return "bool";
		}
		if (FieldValues.IsInteger(type))
		{
			return "whole";
		}
		if (type == typeof(float) || type == typeof(double))
		{
			return "number";
		}
		if (type == typeof(Color))
		{
			return "color";
		}
		return type == typeof(Quaternion) ? "rotation" : type == typeof(Obj) ? "obj" : null;
	}

	/// <summary>Vectors, assets, lists, references to behaviours and components, and [Serializable] types.</summary>
	private static bool WriteStructured(Utf8JsonWriter json, Type type, Dictionary<string, Type> types)
	{
		var size = type == typeof(Vector2) ? 2 : type == typeof(Vector3) ? 3 : type == typeof(Vector4) ? 4 : 0;
		if (size > 0)
		{
			json.WriteString("kind", "vector");
			json.WriteNumber("size", size);
			return true;
		}
		if (FieldValues.IsGeneric(type, typeof(Ref<>)) || FieldValues.IsAsset(type))
		{
			var asset = FieldValues.IsAsset(type) ? type : type.GetGenericArguments()[0];
			json.WriteString("kind", FieldValues.IsAsset(type) ? "asset" : "ref");
			json.WriteString("asset", asset.FullName);
			return true;
		}
		return WriteListOrReference(json, type, types);
	}

	private static bool WriteListOrReference(Utf8JsonWriter json, Type type, Dictionary<string, Type> types)
	{
		var element = type.IsArray ? type.GetElementType() : FieldValues.IsGeneric(type, typeof(List<>)) ? type.GetGenericArguments()[0] : null;
		if (element != null)
		{
			json.WriteString("kind", "list");
			json.WritePropertyName("element");
			Write(json, element, types);
			return true;
		}
		if (typeof(IObjComponent).IsAssignableFrom(type))
		{
			json.WriteString("kind", "component");
			json.WriteString("name", type.FullName);
			return true;
		}
		if (typeof(Behaviour).IsAssignableFrom(type) || type.IsInterface)
		{
			json.WriteString("kind", "behaviour");
			json.WriteString("name", type.FullName);
			return true;
		}
		if (!FieldValues.IsSerializable(type))
		{
			return false;
		}

		json.WriteString("kind", "struct");
		json.WriteString("name", type.FullName);
		types.TryAdd(type.FullName!, type);
		return true;
	}
}
