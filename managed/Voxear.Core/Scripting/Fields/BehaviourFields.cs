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
	/// SpeedLimit or speedLimit.
	/// </summary>
	internal static class BehaviourFields
	{
		private const BindingFlags Declared =
			BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.DeclaredOnly;

		private static readonly Dictionary<Type, Dictionary<string, FieldInfo>> cache =
			new Dictionary<Type, Dictionary<string, FieldInfo>>();

		public static void Apply(Behaviour behaviour, string json)
		{
			var fields = Find(behaviour.GetType());
			using var document = JsonDocument.Parse(json);
			foreach (var property in document.RootElement.EnumerateObject())
			{
				Set(behaviour, fields, property);
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

		private static void Set(Behaviour behaviour, Dictionary<string, FieldInfo> fields, JsonProperty property)
		{
			var owner = $"{behaviour.GetType().Name} on {behaviour.Obj.Name}";
			if (!fields.TryGetValue(Normalize(property.Name), out var field))
			{
				Log.Warning($"{owner} has no field named {property.Name} that scenes can set, so the value is ignored.");
				return;
			}

			try
			{
				field.SetValue(behaviour, FieldValues.Read(property.Value, field.FieldType));
			}
			catch (Exception exception) when (IsBadValue(exception))
			{
				Log.Warning($"{owner}: {field.Name} cannot be set to {property.Value.GetRawText()}. {exception.Message}");
			}
		}

		/// <summary>The exceptions that mean the value does not fit the field, as opposed to a bug.</summary>
		private static bool IsBadValue(Exception exception)
		{
			return exception is FormatException or InvalidOperationException or OverflowException
				or NotSupportedException or ArgumentException;
		}

		private static Dictionary<string, FieldInfo> Find(Type type)
		{
			if (cache.TryGetValue(type, out var found))
			{
				return found;
			}

			found = new Dictionary<string, FieldInfo>();
			for (var current = type; current != null && current != typeof(Behaviour); current = current.BaseType)
			{
				AddSettable(current, found);
			}
			cache[type] = found;
			return found;
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
