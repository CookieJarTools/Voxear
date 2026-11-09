using System;
using System.Collections.Generic;
using System.Reflection;

namespace Voxear.Scripting;

/// <summary>
/// The drawers an assembly holds, and which of them draw a field: one for each of its attributes that has one, in
/// the order they are written, then the one for its type. A list's drawers draw each of its items, so they are
/// matched against the item's type. The catalog writes each field's drawers by name, outermost first, and each
/// behaviour's and Definition's inspector.
/// </summary>
internal sealed class DrawerTable
{
	private readonly Dictionary<Type, Type> values = new Dictionary<Type, Type>();
	private readonly List<(Type Attribute, Type Value, Type Drawer)> attributes = new List<(Type, Type, Type)>();
	private readonly Dictionary<Type, Type> inspectors = new Dictionary<Type, Type>();

	public static DrawerTable Of(IEnumerable<Type?> types)
	{
		var table = new DrawerTable();
		foreach (var type in types)
		{
			if (type != null && Usable(type) && KindOf(type) is { } kind)
			{
				table.Add(type, kind.Attribute, kind.Value);
			}
			if (type != null && Usable(type) && Inspects(type) is { } behaviour)
			{
				table.inspectors.TryAdd(behaviour, type);
			}
		}
		return table;
	}

	/// <summary>The attribute a drawer type draws for, if it is an AttributeDrawer, and the type of value it draws.</summary>
	public static (Type? Attribute, Type Value)? KindOf(Type drawer)
	{
		for (var type = drawer.BaseType; type != null; type = type.BaseType)
		{
			if (FieldValues.IsGeneric(type, typeof(ValueDrawer<>)))
			{
				return (null, type.GetGenericArguments()[0]);
			}
			if (FieldValues.IsGeneric(type, typeof(AttributeDrawer<,>)))
			{
				var arguments = type.GetGenericArguments();
				return (arguments[0], arguments[1]);
			}
		}
		return null;
	}

	/// <summary>The behaviour or Definition type an inspector draws, if it is a BehaviourInspector or a DefinitionInspector.</summary>
	public static Type? Inspects(Type inspector)
	{
		for (var type = inspector.BaseType; type != null; type = type.BaseType)
		{
			if (FieldValues.IsGeneric(type, typeof(BehaviourInspector<>)) || FieldValues.IsGeneric(type, typeof(DefinitionInspector<>)))
			{
				return type.GetGenericArguments()[0];
			}
		}
		return null;
	}

	/// <summary>The inspector that draws a behaviour or a Definition: its own type's, or the nearest base class's.</summary>
	public Type? Inspector(Type behaviour)
	{
		for (var type = behaviour; type != null; type = type.BaseType)
		{
			if (inspectors.TryGetValue(type, out var inspector))
			{
				return inspector;
			}
		}
		return null;
	}

	/// <summary>The type of a list's items, or null for a value that is not a list.</summary>
	public static Type? ItemType(Type type)
	{
		if (type.IsArray)
		{
			return type.GetElementType();
		}
		return FieldValues.IsGeneric(type, typeof(List<>)) ? type.GetGenericArguments()[0] : null;
	}

	/// <summary>The drawers of a field whose value, or each of whose items, is of type value, outermost first.</summary>
	public List<Type> Chain(FieldInfo field, Type value)
	{
		var chain = new List<Type>();
		foreach (var attribute in field.GetCustomAttributes(inherit: true))
		{
			if (ForAttribute(attribute.GetType(), value) is { } drawer)
			{
				chain.Add(drawer);
			}
		}
		for (var type = value; type != null; type = type.BaseType)
		{
			if (values.TryGetValue(type, out var drawer))
			{
				chain.Add(drawer);
				break;
			}
		}
		return chain;
	}

	/// <summary>A drawer the Inspector can make: not abstract or generic, and made with nothing.</summary>
	private static bool Usable(Type type)
	{
		return !type.IsAbstract && !type.ContainsGenericParameters
			&& type.GetConstructor(BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic, Type.EmptyTypes) != null;
	}

	private void Add(Type drawer, Type? attribute, Type value)
	{
		if (attribute == null)
		{
			values.TryAdd(value, drawer);
		}
		else
		{
			attributes.Add((attribute, value, drawer));
		}
	}

	private Type? ForAttribute(Type attribute, Type value)
	{
		foreach (var entry in attributes)
		{
			if (entry.Attribute == attribute && entry.Value.IsAssignableFrom(value))
			{
				return entry.Drawer;
			}
		}
		return null;
	}
}
