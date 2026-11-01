using System;
using System.Collections.Generic;
using System.Reflection;
using System.Runtime.CompilerServices;

namespace Voxear.Saving
{
	/// <summary>
	/// The fields and properties a save keeps of a type: its public fields and the private ones marked
	/// [SerializeField], as scenes set them, readonly ones too; and its public properties that can be set, init-only
	/// ones included, as records have. [NonSerialized] or [DontSave] leaves one out, and so does a delegate, which
	/// is code rather than a value. An auto-property marked [field: SerializeField] is kept by its own name. Base
	/// classes' members come first, as the Inspector shows them; the engine's own base classes have none.
	/// </summary>
	internal static class SaveMembers
	{
		private const BindingFlags Declared =
			BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.DeclaredOnly;

		private static readonly Dictionary<Type, List<SaveMember>> cache = new Dictionary<Type, List<SaveMember>>();

		public static IReadOnlyList<SaveMember> Of(Type type)
		{
			if (cache.TryGetValue(type, out var found))
			{
				return found;
			}

			found = new List<SaveMember>();
			var names = new HashSet<string>();
			var levels = new List<Type>();
			for (var current = type; current != null && !IsBase(current); current = current.BaseType)
			{
				levels.Insert(0, current);
			}
			foreach (var level in levels)
			{
				AddLevel(level, found, names);
			}
			if (found.Count == 0 && !type.IsValueType && HasFields(levels))
			{
				Log.Warning($"A save keeps nothing of {type.Name}: its fields are private. Mark the ones to keep [SerializeField], or make them public.");
			}
			cache[type] = found;
			return found;
		}

		public static void Clear()
		{
			cache.Clear();
		}

		private static void AddLevel(Type level, List<SaveMember> found, HashSet<string> names)
		{
			foreach (var field in level.GetFields(Declared))
			{
				if (IsKept(field) && names.Add(NameOf(field)))
				{
					found.Add(new SaveMember(NameOf(field), field));
				}
			}
			foreach (var property in level.GetProperties(Declared))
			{
				if (IsKept(property) && names.Add(property.Name))
				{
					found.Add(new SaveMember(property));
				}
			}
		}

		private static bool IsKept(FieldInfo field)
		{
			if (field.IsLiteral || field.IsDefined(typeof(NonSerializedAttribute)) || field.IsDefined(typeof(DontSaveAttribute)) || IsCode(field.FieldType))
			{
				return false;
			}
			return (field.IsPublic && !field.IsDefined(typeof(CompilerGeneratedAttribute))) || field.IsDefined(typeof(SerializeFieldAttribute));
		}

		private static bool IsKept(PropertyInfo property)
		{
			var getter = property.GetMethod;
			return getter != null && getter.IsPublic && property.SetMethod != null && property.GetIndexParameters().Length == 0
				&& !property.IsDefined(typeof(DontSaveAttribute)) && !IsCode(property.PropertyType);
		}

		/// <summary>A backing field marked [field: SerializeField] goes by its property's name.</summary>
		private static string NameOf(FieldInfo field)
		{
			var name = field.Name;
			return name.StartsWith('<') && name.IndexOf('>') is var end and > 1 ? name[1..end] : name;
		}

		private static bool IsCode(Type type)
		{
			return typeof(Delegate).IsAssignableFrom(type) || type.IsPointer || type.IsByRef;
		}

		private static bool HasFields(List<Type> levels)
		{
			return levels.Exists(level => level.GetFields(Declared).Length > 0);
		}

		/// <summary>Where the search stops: the engine's own base classes have nothing for a save to keep.</summary>
		private static bool IsBase(Type type)
		{
			return type == typeof(Behaviour) || type == typeof(object) || type == typeof(ValueType);
		}
	}
}
