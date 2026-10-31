using System;
using System.Collections.Generic;
using System.Reflection;
using System.Text.Json;

namespace Voxear.Scripting
{
	/// <summary>
	/// Sets behaviour fields from the values a scene or prefab gives them, just before Awake, which is when Unity has
	/// finished deserialising too. A field can be set when it is public, or private with [SerializeField], and is
	/// neither readonly nor [NonSerialized]. Names match ignoring case and underscores, so speed_limit in a scene sets
	/// SpeedLimit or speedLimit. The same rules fill [Serializable] classes and structs nested inside behaviours.
	/// </summary>
	internal static class BehaviourFields
	{
		private const BindingFlags Declared =
			BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.DeclaredOnly;

		private static readonly Dictionary<Type, Dictionary<string, FieldInfo>> cache =
			new Dictionary<Type, Dictionary<string, FieldInfo>>();

		public static void Apply(Behaviour behaviour, string json)
		{
			using var document = JsonDocument.Parse(json);
			Fill(behaviour, document.RootElement, $"{behaviour.GetType().Name} on {behaviour.Obj.Name}");
		}

		/// <summary>
		/// Sets the fields of target from a table of values. path names where target sits, such as
		/// "Spawner on Wolves: waves[1]", so warnings say exactly which value did not fit.
		/// </summary>
		public static void Fill(object target, JsonElement table, string path)
		{
			var fields = Find(target.GetType());
			foreach (var property in table.EnumerateObject())
			{
				Set(target, fields, property, path);
			}
		}

		/// <summary>Forgets every type, so reloaded game scripts are not held in memory by old entries.</summary>
		public static void Clear()
		{
			cache.Clear();
		}

		public static string Normalize(string name)
		{
			return name.Replace("_", string.Empty).ToLowerInvariant();
		}

		private static void Set(object target, Dictionary<string, FieldInfo> fields, JsonProperty property, string path)
		{
			if (!fields.TryGetValue(Normalize(property.Name), out var field))
			{
				Log.Warning($"{path} has no field named {property.Name} that scenes can set, so the value is ignored.");
				return;
			}

			try
			{
				field.SetValue(target, FieldValues.Read(property.Value, field.FieldType, $"{path}: {field.Name}"));
			}
			catch (Exception exception) when (IsBadValue(exception))
			{
				Log.Warning($"{path}: {field.Name} cannot be set to {property.Value.GetRawText()}. {exception.Message}");
			}
		}

		/// <summary>The exceptions that mean the value does not fit the field, as opposed to a bug.</summary>
		public static bool IsBadValue(Exception exception)
		{
			return exception is FormatException or InvalidOperationException or OverflowException
				or NotSupportedException or ArgumentException or MissingMethodException;
		}

		private static Dictionary<string, FieldInfo> Find(Type type)
		{
			if (cache.TryGetValue(type, out var found))
			{
				return found;
			}

			found = new Dictionary<string, FieldInfo>();
			for (var current = type; current != null && !IsBase(current); current = current.BaseType)
			{
				AddSettable(current, found);
			}
			cache[type] = found;
			return found;
		}

		/// <summary>
		/// The fields scenes can set on a type, base classes' first and each in the order declared, as the editor shows
		/// them.
		/// </summary>
		public static List<FieldInfo> Settable(Type type)
		{
			var levels = new List<Type>();
			for (var current = type; current != null && !IsBase(current); current = current.BaseType)
			{
				levels.Insert(0, current);
			}

			var fields = new List<FieldInfo>();
			foreach (var level in levels)
			{
				fields.AddRange(Array.FindAll(level.GetFields(Declared), IsSettable));
			}
			return fields;
		}

		/// <summary>Private fields of base classes are only visible from their own type, so each level is read in turn.</summary>
		private static void AddSettable(Type type, Dictionary<string, FieldInfo> found)
		{
			foreach (var field in type.GetFields(Declared))
			{
				if (IsSettable(field))
				{
					found.TryAdd(Normalize(field.Name), field);
				}
			}
		}

		/// <summary>Where the search for fields stops: the engine's own base classes have none for scenes to set.</summary>
		private static bool IsBase(Type type)
		{
			return type == typeof(Behaviour) || type == typeof(object) || type == typeof(ValueType);
		}

		private static bool IsSettable(FieldInfo field)
		{
			if (field.IsInitOnly || field.IsLiteral || field.IsDefined(typeof(NonSerializedAttribute)))
			{
				return false;
			}
			return field.IsPublic || field.IsDefined(typeof(SerializeFieldAttribute));
		}
	}
}
