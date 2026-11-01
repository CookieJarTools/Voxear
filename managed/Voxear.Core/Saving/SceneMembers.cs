using System;
using System.Collections.Generic;
using System.Reflection;

namespace Voxear.Saving
{
	/// <summary>
	/// The members a save of the scene keeps of a behaviour: those marked [Saved], private ones too, or, for a class
	/// marked [Saved], every member a save keeps of it (see SaveMembers).
	/// </summary>
	internal static class SceneMembers
	{
		private const BindingFlags Declared =
			BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.DeclaredOnly;

		private static readonly Dictionary<Type, IReadOnlyList<SaveMember>> cache = new Dictionary<Type, IReadOnlyList<SaveMember>>();

		public static IReadOnlyList<SaveMember> Of(Type type)
		{
			if (!cache.TryGetValue(type, out var found))
			{
				cache[type] = found = IsMarked(type) ? SaveMembers.Of(type) : Marked(type);
			}
			return found;
		}

		public static void Clear()
		{
			cache.Clear();
		}

		private static bool IsMarked(Type type)
		{
			for (var current = type; current != null && !SaveMembers.IsBase(current); current = current.BaseType)
			{
				if (current.IsDefined(typeof(SavedAttribute), false))
				{
					return true;
				}
			}
			return false;
		}

		private static List<SaveMember> Marked(Type type)
		{
			var levels = new List<Type>();
			for (var current = type; current != null && !SaveMembers.IsBase(current); current = current.BaseType)
			{
				levels.Insert(0, current);
			}

			var found = new List<SaveMember>();
			foreach (var level in levels)
			{
				foreach (var field in level.GetFields(Declared))
				{
					if (field.IsDefined(typeof(SavedAttribute)) && Keeps(type, field.Name, field.FieldType, !field.IsLiteral))
					{
						found.Add(new SaveMember(SaveMembers.NameOf(field), field));
					}
				}
				foreach (var property in level.GetProperties(Declared))
				{
					var settable = property.GetMethod != null && property.SetMethod != null && property.GetIndexParameters().Length == 0;
					if (property.IsDefined(typeof(SavedAttribute)) && Keeps(type, property.Name, property.PropertyType, settable))
					{
						found.Add(new SaveMember(property));
					}
				}
			}
			return found;
		}

		private static bool Keeps(Type type, string name, Type valueType, bool settable)
		{
			if (!settable)
			{
				Log.Warning($"{type.Name}.{name} is marked [Saved], but cannot be set, so a save of the scene cannot put it back.");
				return false;
			}
			if (SaveMembers.IsCode(valueType))
			{
				Log.Warning($"{type.Name}.{name} is marked [Saved], but a delegate is code, not a value, so a save cannot keep it.");
				return false;
			}
			return true;
		}
	}
}
