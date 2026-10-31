using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using System.Text;
using System.Text.Json;

namespace Voxear.Scripting
{
	/// <summary>
	/// Describes the behaviours of a built game assembly for the editor, as JSON: each behaviour a scene can add, the
	/// types it derives from and implements, and the fields BehaviourFields can set on it, with the kind of value each
	/// takes and the value a new instance starts with. IComponent structs are described the same way, as components. The editor reads this rather than the source, so what it shows
	/// is what the compiler made. [Serializable] types those fields use are described once each, by name, and every
	/// [Command] method is listed for the palette.
	/// </summary>
	internal static class ScriptCatalog
	{
		private const BindingFlags Static = BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.DeclaredOnly;

		public static string Describe(Assembly assembly)
		{
			var nested = new SortedSet<string>(StringComparer.Ordinal);
			var types = new Dictionary<string, Type>();
			using var stream = new MemoryStream();
			using (var json = new Utf8JsonWriter(stream, new JsonWriterOptions { Indented = true }))
			{
				json.WriteStartObject();
				json.WriteStartArray("behaviours");
				foreach (var behaviour in Behaviours(assembly))
				{
					WriteBehaviour(json, behaviour, types);
				}
				json.WriteEndArray();
				json.WriteStartArray("components");
				foreach (var component in Components(assembly))
				{
					WriteComponent(json, component, types);
				}
				json.WriteEndArray();
				WriteNestedTypes(json, types, nested);
				WriteCommands(json, assembly);
				json.WriteEndObject();
			}
			return Encoding.UTF8.GetString(stream.ToArray());
		}

		/// <summary>Every behaviour a scene can name: not abstract, and not generic.</summary>
		private static List<Type> Behaviours(Assembly assembly)
		{
			var found = new List<Type>();
			foreach (var type in Loadable(assembly))
			{
				if (type != null && typeof(Behaviour).IsAssignableFrom(type) && !type.IsAbstract && !type.ContainsGenericParameters)
				{
					found.Add(type);
				}
			}
			found.Sort((a, b) => string.CompareOrdinal(a.FullName, b.FullName));
			return found;
		}

		/// <summary>Every component a scene can give an Obj: an unmanaged struct marked IComponent.</summary>
		private static List<Type> Components(Assembly assembly)
		{
			var found = new List<Type>();
			foreach (var type in Loadable(assembly))
			{
				if (type != null && SceneComponentTypes.IsComponent(type))
				{
					found.Add(type);
				}
			}
			found.Sort((a, b) => string.CompareOrdinal(a.FullName, b.FullName));
			return found;
		}

		/// <summary>A component, as a behaviour is described but with no bases: its fields start as the struct's zero value.</summary>
		private static void WriteComponent(Utf8JsonWriter json, Type type, Dictionary<string, Type> types)
		{
			json.WriteStartObject();
			json.WriteString("name", type.FullName);
			json.WriteStartArray("bases");
			json.WriteEndArray();
			json.WriteStartArray("interfaces");
			foreach (var contract in type.GetInterfaces())
			{
				json.WriteStringValue(contract.FullName);
			}
			json.WriteEndArray();
			WriteFields(json, type, Activator.CreateInstance(type), types, ComponentLayout.Of(type));
			json.WriteEndObject();
		}

		/// <summary>The assembly's types, leaving out any that cannot load rather than failing for all of them.</summary>
		private static Type?[] Loadable(Assembly assembly)
		{
			try
			{
				return assembly.GetTypes();
			}
			catch (ReflectionTypeLoadException exception)
			{
				return exception.Types;
			}
		}

		private static void WriteBehaviour(Utf8JsonWriter json, Type type, Dictionary<string, Type> types)
		{
			json.WriteStartObject();
			json.WriteString("name", type.FullName);
			json.WriteStartArray("bases");
			for (var current = type.BaseType; current != null && current != typeof(Behaviour); current = current.BaseType)
			{
				json.WriteStringValue(current.FullName);
			}
			json.WriteEndArray();
			json.WriteStartArray("interfaces");
			foreach (var contract in type.GetInterfaces())
			{
				json.WriteStringValue(contract.FullName);
			}
			json.WriteEndArray();
			WriteFields(json, type, FieldDefaults.Create(type), types);
			json.WriteEndObject();
		}

		/// <summary>
		/// The fields scenes can set, each with its kind and the value it starts with in instance, and, for a component
		/// whose layout is known, where it lies in its bytes.
		/// </summary>
		public static void WriteFields(Utf8JsonWriter json, Type type, object? instance, Dictionary<string, Type> types, Dictionary<FieldInfo, int>? layout = null)
		{
			json.WriteStartArray("fields");
			foreach (var field in BehaviourFields.Settable(type))
			{
				json.WriteStartObject();
				json.WriteString("name", field.Name);
				json.WritePropertyName("type");
				FieldKinds.Write(json, field.FieldType, types);
				json.WritePropertyName("default");
				FieldDefaults.Write(json, instance == null ? null : field.GetValue(instance), field.FieldType);
				InspectorMetadata.Write(json, field);
				if (layout != null && layout.TryGetValue(field, out var offset))
				{
					WriteLayout(json, field, offset);
				}
				json.WriteEndObject();
			}
			json.WriteEndArray();
		}

		/// <summary>Where a component's field lies: its offset and size in bytes, and whether a whole number is signed.</summary>
		private static void WriteLayout(Utf8JsonWriter json, FieldInfo field, int offset)
		{
			var (size, signed) = ComponentLayout.Value(field.FieldType);
			json.WriteStartObject("layout");
			json.WriteNumber("offset", offset);
			json.WriteNumber("size", size);
			json.WriteBoolean("signed", signed);
			json.WriteEndObject();
		}

		/// <summary>
		/// Each [Serializable] type the fields use, by name. Describing one can find more, so this goes on until every
		/// type found has been written.
		/// </summary>
		private static void WriteNestedTypes(Utf8JsonWriter json, Dictionary<string, Type> types, SortedSet<string> written)
		{
			json.WriteStartObject("types");
			while (Next(types, written) is { } type)
			{
				written.Add(type.FullName!);
				json.WriteStartObject(type.FullName!);
				WriteFields(json, type, FieldDefaults.Create(type), types);
				json.WriteEndObject();
			}
			json.WriteEndObject();
		}

		/// <summary>Each static method marked [Command] that takes nothing and returns nothing, as the palette runs it.</summary>
		private static void WriteCommands(Utf8JsonWriter json, Assembly assembly)
		{
			json.WriteStartArray("commands");
			foreach (var type in Loadable(assembly))
			{
				if (type == null)
				{
					continue;
				}
				foreach (var method in type.GetMethods(Static))
				{
					WriteCommand(json, type, method);
				}
			}
			json.WriteEndArray();
		}

		private static void WriteCommand(Utf8JsonWriter json, Type type, MethodInfo method)
		{
			var command = method.GetCustomAttribute<CommandAttribute>();
			if (command == null || method.GetParameters().Length > 0 || method.ReturnType != typeof(void))
			{
				return;
			}

			json.WriteStartObject();
			json.WriteString("title", command.Title);
			json.WriteString("keys", command.Keys);
			json.WriteString("type", type.FullName);
			json.WriteString("method", method.Name);
			json.WriteEndObject();
		}

		private static Type? Next(Dictionary<string, Type> types, SortedSet<string> written)
		{
			foreach (var (name, type) in types)
			{
				if (!written.Contains(name))
				{
					return type;
				}
			}
			return null;
		}
	}
}
