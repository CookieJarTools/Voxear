using System;
using System.Collections.Generic;
using System.Reflection;
using Voxear.Scripting;

namespace Voxear.Saving
{
	/// <summary>
	/// Lists, arrays and the other collections a save keeps as lists: anything with items of one type that can be
	/// made empty and added to. A declared interface, such as IReadOnlyList&lt;T&gt;, reads back as a List&lt;T&gt;,
	/// and a set's as a HashSet&lt;T&gt;. A Stack reads back with the same item on top.
	/// </summary>
	internal static class Collections
	{
		/// <summary>The type of the items, or null for a type that is not a collection a save keeps.</summary>
		public static Type? Element(Type type)
		{
			if (type.IsArray)
			{
				return type.GetArrayRank() == 1 ? type.GetElementType() : null;
			}
			if (type == typeof(string))
			{
				return null;
			}
			var enumerable = type.IsInterface && FieldValues.IsGeneric(type, typeof(IEnumerable<>))
				? type
				: Array.Find(type.GetInterfaces(), contract => FieldValues.IsGeneric(contract, typeof(IEnumerable<>)));
			return enumerable?.GetGenericArguments()[0];
		}

		/// <summary>Makes the collection from its items, as the declared type wants it.</summary>
		public static object Make(Type type, Type element, List<object?> items)
		{
			if (type.IsArray)
			{
				var array = Array.CreateInstance(element, items.Count);
				for (var at = 0; at < items.Count; at++)
				{
					array.SetValue(items[at], at);
				}
				return array;
			}

			var concrete = Concrete(type, element);
			var made = Activator.CreateInstance(concrete, nonPublic: true)
				?? throw new NotSupportedException($"A {type.Name} cannot be made empty to fill.");
			if (FieldValues.IsGeneric(concrete, typeof(Stack<>)))
			{
				items.Reverse();
			}
			var add = Adder(concrete, element) ?? throw new NotSupportedException($"A {type.Name} has no way to add items to it.");
			foreach (var item in items)
			{
				add.Invoke(made, new[] { item });
			}
			return made;
		}

		private static Type Concrete(Type type, Type element)
		{
			if (!type.IsInterface && !type.IsAbstract)
			{
				return type;
			}

			var set = typeof(HashSet<>).MakeGenericType(element);
			return type.IsAssignableFrom(set) && type.Name.Contains("Set", StringComparison.Ordinal) ? set : typeof(List<>).MakeGenericType(element);
		}

		private static MethodInfo? Adder(Type type, Type element)
		{
			foreach (var name in new[] { "Add", "Push", "Enqueue" })
			{
				var method = type.GetMethod(name, BindingFlags.Instance | BindingFlags.Public, new[] { element });
				if (method != null)
				{
					return method;
				}
			}
			var collection = typeof(ICollection<>).MakeGenericType(element);
			return collection.IsAssignableFrom(type) ? collection.GetMethod("Add") : null;
		}
	}
}
