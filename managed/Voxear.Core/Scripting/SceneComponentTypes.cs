using System;
using System.Collections.Generic;
using System.Reflection;
using System.Text.Json;

namespace Voxear.Scripting
{
	/// <summary>
	/// Components that scenes and prefabs name, made from their field values. The type is only known by name here, so
	/// the value is filled in boxed, by the same rules as a behaviour's fields, and added through Entity.Add made for
	/// its type, which is found once per type and kept until the scripts reload.
	/// </summary>
	internal static class SceneComponentTypes
	{
		private static readonly MethodInfo add = typeof(SceneComponentTypes).GetMethod(nameof(AddTyped), BindingFlags.NonPublic | BindingFlags.Static)!;

		private static readonly Dictionary<string, (Type Type, MethodInfo Add)?> found = new Dictionary<string, (Type, MethodInfo)?>();

		public static bool Add(string typeName, Entity entity, string? fields)
		{
			if (Find(typeName) is not { } component)
			{
				Log.Error($"{typeName} is not an IComponent struct in the game's scripts, so {entity.Obj.Name} goes without it.");
				return false;
			}

			var value = Activator.CreateInstance(component.Type)!;
			if (fields != null)
			{
				using var document = JsonDocument.Parse(fields);
				BehaviourFields.Fill(value, document.RootElement, $"{component.Type.Name} on {entity.Obj.Name}");
			}
			component.Add.Invoke(null, new[] { entity, value });
			return true;
		}

		/// <summary>Forgets every type, so reloaded game scripts are not held in memory.</summary>
		public static void Clear()
		{
			found.Clear();
		}

		/// <summary>Whether a type is one scenes can give an Obj: an unmanaged struct marked IComponent.</summary>
		public static bool IsComponent(Type type)
		{
			return type.IsValueType && !type.IsEnum && typeof(IComponent).IsAssignableFrom(type) && !type.ContainsGenericParameters
				&& IsUnmanaged(type);
		}

		/// <summary>Whether a type holds no references anywhere inside it, which is what C#'s unmanaged constraint asks.</summary>
		private static bool IsUnmanaged(Type type)
		{
			if (type.IsPrimitive || type.IsPointer || type.IsEnum)
			{
				return true;
			}
			if (!type.IsValueType)
			{
				return false;
			}
			return Array.TrueForAll(type.GetFields(BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic), field => IsUnmanaged(field.FieldType));
		}

		private static (Type Type, MethodInfo Add)? Find(string typeName)
		{
			if (found.TryGetValue(typeName, out var known))
			{
				return known;
			}

			known = null;
			foreach (var assembly in GameScripts.Assemblies)
			{
				if (assembly.GetType(typeName) is { } type && IsComponent(type))
				{
					known = (type, add.MakeGenericMethod(type));
					break;
				}
			}
			found[typeName] = known;
			return known;
		}

		private static void AddTyped<T>(Entity entity, object value) where T : unmanaged, IComponent
		{
			entity.Add((T)value);
		}
	}
}
