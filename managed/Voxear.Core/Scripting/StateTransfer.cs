using System;
using System.Collections;
using System.Collections.Generic;
using System.Globalization;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.Loader;

namespace Voxear.Scripting;

/// <summary>
/// Carries what behaviours hold across a reload of the game scripts. Every field, private ones and the ones behind
/// properties too, goes into the field of the same name on the new instance when its value still fits there.
/// Values of the old scripts' own types are made again as the new scripts' types of the same names: classes and
/// structs field by field, enums by name, arrays, lists and dictionaries item by item, and behaviours as the
/// behaviours that replace them. Something reached twice is made again once, so shared and circular references
/// stay as they were. What cannot be carried, such as a delegate into the old code or a field whose type changed
/// beyond converting, keeps the value the new instance starts with, and is listed in Lost.
/// </summary>
internal sealed class StateTransfer
{
	private const BindingFlags DeclaredFields =
		BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.DeclaredOnly;

	/// <summary>Each old behaviour and the one that takes its place, filled in before any field is carried.</summary>
	public Dictionary<Behaviour, Behaviour> Replaced { get; } = new Dictionary<Behaviour, Behaviour>(ReferenceEqualityComparer.Instance);

	/// <summary>Where a value could not be carried over, such as "Spinner on Spinner A: onHit".</summary>
	public List<string> Lost { get; } = new List<string>();

	private readonly AssemblyLoadContext old;
	private readonly IReadOnlyList<Assembly> fresh;
	private readonly Dictionary<object, object?> remade = new Dictionary<object, object?>(ReferenceEqualityComparer.Instance);

	public StateTransfer(AssemblyLoadContext old, IReadOnlyList<Assembly> fresh)
	{
		this.old = old;
		this.fresh = fresh;
	}

	/// <summary>Copies the fields of an old instance into the new one, matched by name.</summary>
	public void Copy(object from, object to, string path)
	{
		var targets = FieldsByName(to.GetType());
		foreach (var field in Fields(from.GetType()))
		{
			if (targets.TryGetValue(field.Name, out var target))
			{
				Move(field.GetValue(from), to, target, $"{path}: {Shown(field.Name)}");
			}
		}
	}

	private void Move(object? value, object to, FieldInfo target, string path)
	{
		if (TryRemake(value, target.FieldType, path, out var made))
		{
			target.SetValue(to, made);
			return;
		}
		Lost.Add(path);
	}

	private bool TryRemake(object? value, Type target, string path, out object? made)
	{
		made = null;
		if (value == null)
		{
			return !target.IsValueType || Nullable.GetUnderlyingType(target) != null;
		}
		if (value is Behaviour behaviour)
		{
			// A behaviour destroyed before the reload has no replacement, and reads as gone, as it did.
			made = Replaced.GetValueOrDefault(behaviour);
			return made == null || target.IsInstanceOfType(made);
		}
		if (value is Delegate && GameCode.Holds(value, old))
		{
			return false;
		}
		if (IsPlain(value.GetType()))
		{
			return Fits(value, target, out made);
		}
		made = Remake(value, path);
		return made != null && target.IsInstanceOfType(made);
	}

	/// <summary>A value with nothing of the old scripts in it, which is carried as it is.</summary>
	private bool IsPlain(Type type)
	{
		return type.IsPrimitive || type == typeof(string) || type == typeof(decimal)
			|| (type.IsValueType && !GameCode.IsFrom(type, old));
	}

	/// <summary>The value as the target type, converting a number or text when a field's type changed between builds.</summary>
	private static bool Fits(object value, Type target, out object? made)
	{
		made = value;
		if (target.IsInstanceOfType(value))
		{
			return true;
		}

		var plain = Nullable.GetUnderlyingType(target) ?? target;
		if (value is not IConvertible || !(plain.IsPrimitive || plain == typeof(string) || plain == typeof(decimal)))
		{
			return false;
		}
		try
		{
			made = Convert.ChangeType(value, plain, CultureInfo.InvariantCulture);
			return true;
		}
		catch (Exception exception) when (exception is InvalidCastException or FormatException or OverflowException)
		{
			return false;
		}
	}

	private object? Remake(object value, string path)
	{
		if (remade.TryGetValue(value, out var done))
		{
			return done;
		}

		var type = value.GetType();
		if (value is Array array)
		{
			return RemakeArray(array, type, path);
		}
		if (FieldValues.IsGeneric(type, typeof(List<>)) || FieldValues.IsGeneric(type, typeof(Dictionary<,>)))
		{
			return RemakeCollection(value, type, path);
		}
		// The engine's and .NET's own objects are kept as they are.
		if (!GameCode.IsFrom(type, old))
		{
			return value;
		}
		return type.IsEnum ? RemakeEnum(value, type) : RemakeObject(value, type, path);
	}

	private object? RemakeObject(object value, Type type, string path)
	{
		var fresh = Map(type);
		if (fresh == null)
		{
			return null;
		}

		var made = FieldDefaults.Create(fresh) ?? RuntimeHelpers.GetUninitializedObject(fresh);
		remade[value] = made;
		Copy(value, made, path);
		return made;
	}

	/// <summary>An enum value by its name, or by its number when the new build has no such name.</summary>
	private object? RemakeEnum(object value, Type type)
	{
		var fresh = Map(type);
		if (fresh == null)
		{
			return null;
		}

		var name = value.ToString();
		if (name != null && Enum.TryParse(fresh, name, out var named))
		{
			return named;
		}
		return Enum.ToObject(fresh, Convert.ChangeType(value, Enum.GetUnderlyingType(type), CultureInfo.InvariantCulture));
	}

	private object? RemakeArray(Array array, Type type, string path)
	{
		var element = Map(type.GetElementType()!);
		if (element == null || array.Rank != 1)
		{
			return GameCode.IsFrom(type, old) ? null : array;
		}

		var made = Array.CreateInstance(element, array.Length);
		remade[array] = made;
		for (var i = 0; i < array.Length; i++)
		{
			made.SetValue(Item(array.GetValue(i), element, $"{path}[{i}]"), i);
		}
		return made;
	}

	private object? RemakeCollection(object value, Type type, string path)
	{
		var fresh = Map(type);
		if (fresh == null || Activator.CreateInstance(fresh) is not { } made)
		{
			return null;
		}

		remade[value] = made;
		var arguments = fresh.GetGenericArguments();
		if (made is IDictionary table)
		{
			FillTable((IDictionary)value, table, arguments[0], arguments[1], path);
			return made;
		}

		var list = (IList)made;
		var index = 0;
		foreach (var item in (IList)value)
		{
			list.Add(Item(item, arguments[0], $"{path}[{index++}]"));
		}
		return made;
	}

	private void FillTable(IDictionary from, IDictionary to, Type keyType, Type valueType, string path)
	{
		foreach (DictionaryEntry entry in from)
		{
			var key = Item(entry.Key, keyType, $"{path}[{entry.Key}]");
			if (key != null)
			{
				to[key] = Item(entry.Value, valueType, $"{path}[{entry.Key}]");
			}
		}
	}

	/// <summary>One item of a collection, or the type's empty value when it cannot be carried.</summary>
	private object? Item(object? value, Type type, string path)
	{
		if (TryRemake(value, type, path, out var made))
		{
			return made;
		}

		Lost.Add(path);
		return type.IsValueType ? Activator.CreateInstance(type) : null;
	}

	/// <summary>The new build's type of the same name, with any generic arguments mapped too.</summary>
	private Type? Map(Type type)
	{
		if (!GameCode.IsFrom(type, old))
		{
			return type;
		}
		if (type.IsArray)
		{
			return Map(type.GetElementType()!)?.MakeArrayType();
		}
		if (type.IsConstructedGenericType)
		{
			return MapGeneric(type);
		}
		return Find(type.FullName);
	}

	private Type? MapGeneric(Type type)
	{
		var definition = Map(type.GetGenericTypeDefinition());
		var arguments = Array.ConvertAll(type.GetGenericArguments(), Map);
		if (definition == null || Array.Exists(arguments, argument => argument == null))
		{
			return null;
		}
		return definition.MakeGenericType(arguments!);
	}

	private Type? Find(string? name)
	{
		if (name == null)
		{
			return null;
		}
		foreach (var assembly in fresh)
		{
			if (assembly.GetType(name) is { } found)
			{
				return found;
			}
		}
		return null;
	}

	/// <summary>Every instance field below the engine's own base classes, the most derived first.</summary>
	private static IEnumerable<FieldInfo> Fields(Type type)
	{
		for (var current = type; current != null && !IsBase(current); current = current.BaseType)
		{
			foreach (var field in current.GetFields(DeclaredFields))
			{
				yield return field;
			}
		}
	}

	/// <summary>The fields by name; where a base class has a private field of the same name, the derived one wins.</summary>
	private static Dictionary<string, FieldInfo> FieldsByName(Type type)
	{
		var found = new Dictionary<string, FieldInfo>();
		foreach (var field in Fields(type))
		{
			found.TryAdd(field.Name, field);
		}
		return found;
	}

	private static bool IsBase(Type type)
	{
		return type == typeof(Behaviour) || type == typeof(object) || type == typeof(ValueType);
	}

	/// <summary>A property's hidden field by the property's name, so a lost value reads as the script names it.</summary>
	private static string Shown(string field)
	{
		var end = field.IndexOf(">k__BackingField", StringComparison.Ordinal);
		return field.StartsWith('<') && end > 0 ? field[1..end] : field;
	}
}
