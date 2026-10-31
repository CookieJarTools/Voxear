using System;
using System.Collections;
using System.Globalization;
using System.Numerics;
using System.Reflection;
using System.Text.Json;

namespace Voxear.Scripting
{
	/// <summary>
	/// The values fields start with, written as a scene would write them, so that the editor can show a field the
	/// scene does not set. They come from a new instance, made as the engine makes one, so field initialisers have
	/// run. References start as nothing and are written as null.
	/// </summary>
	internal static class FieldDefaults
	{
		/// <summary>A new instance of a behaviour or [Serializable] type, or null when one cannot be made.</summary>
		public static object? Create(Type type)
		{
			try
			{
				return Activator.CreateInstance(type, nonPublic: true);
			}
			catch (Exception exception) when (exception is TargetInvocationException or MissingMethodException or MemberAccessException)
			{
				return null;
			}
		}

		public static void Write(Utf8JsonWriter json, object? value, Type type)
		{
			if (value == null)
			{
				json.WriteNullValue();
				return;
			}
			if (!WritePlain(json, value, type) && !WriteStructured(json, value, type))
			{
				json.WriteNullValue();
			}
		}

		private static bool WritePlain(Utf8JsonWriter json, object value, Type type)
		{
			switch (value)
			{
				case string text:
					json.WriteStringValue(text);
					return true;
				case bool on:
					json.WriteBooleanValue(on);
					return true;
				case Enum choice:
					json.WriteStringValue(choice.ToString());
					return true;
				case float or double:
					WriteNumber(json, value);
					return true;
			}
			if (FieldValues.IsInteger(type))
			{
				json.WriteNumberValue(Convert.ToInt64(value, CultureInfo.InvariantCulture));
				return true;
			}
			return false;
		}

		/// <summary>The shortest digits that read back as the same number, so 0.15f is 0.15 rather than 0.150000006.</summary>
		private static void WriteNumber(Utf8JsonWriter json, object value)
		{
			var text = Convert.ToString(value, CultureInfo.InvariantCulture)!;
			json.WriteNumberValue(double.Parse(text, CultureInfo.InvariantCulture));
		}

		private static bool WriteStructured(Utf8JsonWriter json, object value, Type type)
		{
			if (value is Vector2 or Vector3 or Vector4 or Quaternion)
			{
				WriteFloats(json, value);
				return true;
			}
			if (FieldValues.IsGeneric(type, typeof(Ref<>)) || FieldValues.IsAsset(type))
			{
				var path = type.GetProperty("Path")?.GetValue(value) as string;
				WritePath(json, path);
				return true;
			}
			if (value is IList items)
			{
				WriteList(json, items, type);
				return true;
			}
			if (FieldValues.IsSerializable(type) && !typeof(Behaviour).IsAssignableFrom(type))
			{
				WriteTable(json, value, type);
				return true;
			}
			return false;
		}

		private static void WritePath(Utf8JsonWriter json, string? path)
		{
			if (string.IsNullOrEmpty(path))
			{
				json.WriteNullValue();
				return;
			}
			json.WriteStringValue(path);
		}

		/// <summary>A vector as its numbers, and a rotation as its four components, which scenes also accept.</summary>
		private static void WriteFloats(Utf8JsonWriter json, object value)
		{
			float[] floats = value switch
			{
				Vector2 v => new[] { v.X, v.Y },
				Vector3 v => new[] { v.X, v.Y, v.Z },
				Vector4 v => new[] { v.X, v.Y, v.Z, v.W },
				Quaternion q => q == Quaternion.Identity ? new[] { 0f, 0f, 0f } : new[] { q.X, q.Y, q.Z, q.W },
				_ => Array.Empty<float>(),
			};
			json.WriteStartArray();
			foreach (var number in floats)
			{
				WriteNumber(json, number);
			}
			json.WriteEndArray();
		}

		private static void WriteList(Utf8JsonWriter json, IList items, Type type)
		{
			var element = type.IsArray ? type.GetElementType()! : type.GetGenericArguments()[0];
			json.WriteStartArray();
			foreach (var item in items)
			{
				Write(json, item, element);
			}
			json.WriteEndArray();
		}

		private static void WriteTable(Utf8JsonWriter json, object value, Type type)
		{
			json.WriteStartObject();
			foreach (var field in BehaviourFields.Settable(type))
			{
				json.WritePropertyName(field.Name);
				Write(json, field.GetValue(value), field.FieldType);
			}
			json.WriteEndObject();
		}
	}
}
