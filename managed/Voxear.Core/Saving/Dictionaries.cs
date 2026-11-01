using System;
using System.Collections;
using System.Collections.Generic;
using System.Globalization;
using Voxear.Scripting;

namespace Voxear.Saving
{
	/// <summary>
	/// Dictionaries, kept as a table when their keys read as text (strings, enums, whole numbers and GUIDs) and as a
	/// list of [key, value] pairs otherwise. A key starting with $ is written with a second $ in front, so it is never
	/// taken for one of a save's own marks. A declared interface reads back as a Dictionary&lt;K, V&gt;.
	/// </summary>
	internal static class Dictionaries
	{
		public static KeyValuePair<Type, Type>? Element(Type type)
		{
			var contract = type.IsInterface && IsDictionary(type) ? type : Array.Find(type.GetInterfaces(), IsDictionary);
			if (contract == null)
			{
				return null;
			}

			var arguments = contract.GetGenericArguments();
			return new KeyValuePair<Type, Type>(arguments[0], arguments[1]);
		}

		private static bool IsDictionary(Type type)
		{
			return FieldValues.IsGeneric(type, typeof(IDictionary<,>)) || FieldValues.IsGeneric(type, typeof(IReadOnlyDictionary<,>));
		}

		public static bool KeysAreText(Type key)
		{
			return key == typeof(string) || key.IsEnum || key == typeof(Guid) || FieldValues.IsInteger(key);
		}

		public static IEnumerable<(object Key, object? Value)> Entries(IEnumerable dictionary)
		{
			if (dictionary is IDictionary plain)
			{
				foreach (DictionaryEntry entry in plain)
				{
					yield return (entry.Key, entry.Value);
				}
				yield break;
			}
			foreach (var entry in dictionary)
			{
				var type = entry!.GetType();
				yield return (type.GetProperty("Key")!.GetValue(entry)!, type.GetProperty("Value")!.GetValue(entry));
			}
		}

		public static string KeyText(object key)
		{
			var text = Convert.ToString(key, CultureInfo.InvariantCulture) ?? string.Empty;
			return text.StartsWith('$') ? "$" + text : text;
		}

		public static object ParseKey(string text, Type key)
		{
			text = text.StartsWith("$$", StringComparison.Ordinal) ? text[1..] : text;
			if (key == typeof(string))
			{
				return text;
			}
			if (key.IsEnum)
			{
				return Enum.Parse(key, text, ignoreCase: true);
			}
			return key == typeof(Guid) ? Guid.Parse(text) : Convert.ChangeType(text, key, CultureInfo.InvariantCulture);
		}

		/// <summary>Makes the dictionary from its entries, as the declared type wants it.</summary>
		public static object Make(Type type, Type key, Type value, List<(object Key, object? Value)> entries)
		{
			var concrete = type.IsInterface || type.IsAbstract ? typeof(Dictionary<,>).MakeGenericType(key, value) : type;
			var made = Activator.CreateInstance(concrete, nonPublic: true)
				?? throw new NotSupportedException($"A {type.Name} cannot be made empty to fill.");
			var add = typeof(IDictionary<,>).MakeGenericType(key, value);
			if (made is IDictionary plain)
			{
				entries.ForEach(entry => plain[entry.Key] = entry.Value);
			}
			else if (add.IsAssignableFrom(concrete))
			{
				var method = add.GetMethod("Add")!;
				entries.ForEach(entry => method.Invoke(made, new[] { entry.Key, entry.Value }));
			}
			else
			{
				throw new NotSupportedException($"A {type.Name} has no way to add entries to it.");
			}
			return made;
		}
	}
}
