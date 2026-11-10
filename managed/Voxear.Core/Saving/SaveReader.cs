using System;
using System.Collections.Generic;
using System.Globalization;
using System.Reflection;
using System.Runtime.CompilerServices;
using Voxear.Interop;
using Voxear.Scripting;

namespace Voxear.Saving;

/// <summary>
/// Makes values again from SaveNodes, the way SaveWriter took them apart, reading each node as the type it goes
/// into: a whole number reads into a float field, and text that names an enum value into an enum. Before a table is
/// read, its type's migrations bring it up to date, and a table from a newer version than the type's is refused.
/// A class is made with the constructor that takes nothing, so a member the save does not have keeps the value
/// the class starts it with; a class without one, such as a record, is made without running any constructor. A
/// value that does not fit is left as it starts, with a warning naming where it is, so one bad value never costs a
/// player the rest of the save.
/// </summary>
internal sealed class SaveReader
{
	[ThreadStatic]
	private static SaveReader? current;

	private readonly Dictionary<long, object> made = new Dictionary<long, object>();
	private readonly List<string> path = new List<string>();
	private readonly string context;

	private SaveReader(string context)
	{
		this.context = context;
	}

	/// <summary>
	/// Reads a node as type. into, when given, is the object to fill rather than one to make, as Save.ReadInto
	/// fills a behaviour. Inside a converter, the save being read carries on, sharing its references.
	/// </summary>
	public static object? Read(SaveNode node, Type type, object? into, string context = "")
	{
		if (current != null)
		{
			return current.Value(node, type, into);
		}

		using var scope = Begin(context);
		return current!.Value(node, type, into);
	}

	/// <summary>Fills only these members of target from a table, as a save of the scene puts back a behaviour's marked ones.</summary>
	public static void ReadMembers(SaveNode node, object target, IReadOnlyList<SaveMember> members, string context = "")
	{
		using var scope = Begin(context);
		if (node.Kind != SaveNodeKind.Table)
		{
			current!.Warn(new FormatException($"The save has {node.Shown()} where the values of a {target.GetType().Name} were expected."));
			return;
		}
		current!.Fill(node, target, members);
	}

	/// <summary>One save read in several calls, sharing its references, until the scope ends.</summary>
	public static IDisposable Begin(string context)
	{
		if (current != null)
		{
			return new Scope(null);
		}
		current = new SaveReader(context);
		return new Scope(() => current = null);
	}

	private object? Value(SaveNode node, Type type, object? into)
	{
		if (type == typeof(SaveNode))
		{
			return node;
		}
		if (node.Kind == SaveNodeKind.Table && node.Has("$ref"))
		{
			return Referenced(node, type);
		}
		if (node.Kind == SaveNodeKind.Table && node["$type"].Kind == SaveNodeKind.Text)
		{
			type = Actual(node["$type"].AsString()!, type);
			if (node.Has("$value"))
			{
				return Value(node["$value"], type, null);
			}
		}

		var underlying = Nullable.GetUnderlyingType(type);
		if (node.IsNull)
		{
			return type.IsValueType && underlying == null ? into ?? Activator.CreateInstance(type) : null;
		}
		type = underlying ?? type;
		if (SaveTypes.Converter(type) is { } converter)
		{
			return Converted(converter, node);
		}
		if (Plain(node, type) is { } plain)
		{
			return plain;
		}
		return into == null && TryFound(node, type, out var found) ? found : Structured(node, type, into);
	}

	private object? Converted(SaveConverter converter, SaveNode node)
	{
		try
		{
			return converter.ReadObject(node);
		}
		catch (Exception exception) when (exception is not SaveException && !BehaviourFields.IsBadValue(exception))
		{
			throw new SaveException(SaveError.Unsupported, $"{Where()}: {converter.GetType().Name} threw: {exception.Message}", exception);
		}
	}

	/// <summary>The type a "$type" names, which must fit where it goes.</summary>
	private Type Actual(string name, Type declared)
	{
		var found = SaveTypes.Find(name)
			?? throw new SaveException(SaveError.Unsupported, $"{Where()}: the save holds a {name}, which the game no longer has.");
		if (!declared.IsAssignableFrom(found) && Nullable.GetUnderlyingType(declared) != found)
		{
			throw new FormatException($"The save holds a {found.Name}, which does not fit a {declared.Name}.");
		}
		return found;
	}

	private object Referenced(SaveNode node, Type type)
	{
		var id = node["$ref"].AsLong();
		if (!made.TryGetValue(id, out var found))
		{
			throw new FormatException($"The save points to object {id}, which it does not have.");
		}
		return type.IsInstanceOfType(found) ? found : throw new FormatException($"The save points to a {found.GetType().Name}, which does not fit a {type.Name}.");
	}

	private static object? Plain(SaveNode node, Type type)
	{
		if (type == typeof(string))
		{
			return node.AsString();
		}
		if (type == typeof(bool))
		{
			return node.AsBool();
		}
		if (type.IsEnum)
		{
			return ReadEnum(node, type);
		}
		if (type == typeof(ulong))
		{
			return unchecked((ulong)node.AsLong());
		}
		if (FieldValues.IsInteger(type))
		{
			return Convert.ChangeType(node.AsLong(), type, CultureInfo.InvariantCulture);
		}
		return PlainNumber(node, type);
	}

	private static object? PlainNumber(SaveNode node, Type type)
	{
		if (type == typeof(float))
		{
			return node.AsFloat();
		}
		if (type == typeof(double))
		{
			return node.AsDouble();
		}
		if (type == typeof(decimal))
		{
			return node.Kind == SaveNodeKind.Text ? decimal.Parse(node.AsString()!, CultureInfo.InvariantCulture) : (decimal)node.AsDouble();
		}
		if (type == typeof(char))
		{
			var text = node.AsString();
			return string.IsNullOrEmpty(text) ? throw new FormatException("The save has no letter for it.") : text[0];
		}
		return type == typeof(byte[]) ? node.AsBytes() : null;
	}

	/// <summary>A name, matched ignoring case and underscores as scenes match them; names joined by commas for [Flags]; or a number.</summary>
	private static object ReadEnum(SaveNode node, Type type)
	{
		if (node.Kind == SaveNodeKind.Integer)
		{
			return Enum.ToObject(type, node.AsLong());
		}

		var text = node.AsString() ?? string.Empty;
		var wanted = BehaviourFields.Normalize(text);
		foreach (var name in Enum.GetNames(type))
		{
			if (BehaviourFields.Normalize(name) == wanted)
			{
				return Enum.Parse(type, name);
			}
		}
		if (Enum.TryParse(type, text, ignoreCase: true, out var parsed))
		{
			return parsed!;
		}
		throw new FormatException($"{type.Name} has no value named {text}; it has {string.Join(", ", Enum.GetNames(type))}.");
	}

	/// <summary>The engine's things, found again by what the save kept of them. False for any other type.</summary>
	private bool TryFound(SaveNode node, Type type, out object? found)
	{
		found = null;
		if (type == typeof(Obj))
		{
			found = FindObj(node.AsString()!, "an Obj");
		}
		else if (typeof(Behaviour).IsAssignableFrom(type) || (type.IsInterface && node.Kind == SaveNodeKind.Text))
		{
			found = FindBehaviour(node.AsString()!, type);
		}
		else if (FieldValues.IsAsset(type))
		{
			found = type.GetMethod("Load", new[] { typeof(string) })!.Invoke(null, new object?[] { node.AsString()! });
		}
		else if (typeof(Definition).IsAssignableFrom(type) && node.Kind == SaveNodeKind.Text)
		{
			found = Definitions.Load(type, node.AsString());
		}
		else if (FieldValues.IsGeneric(type, typeof(Ref<>)))
		{
			found = Activator.CreateInstance(type, node.AsString()!);
		}
		else
		{
			return false;
		}
		return true;
	}

	/// <summary>A behaviour another value pointed to: the first of its type on its Obj, as scenes find one.</summary>
	private Behaviour? FindBehaviour(string key, Type type)
	{
		var obj = FindObj(key, $"a {type.Name}");
		var found = obj.IsValid ? obj.GetBehaviour(type) : null;
		if (obj.IsValid && found == null)
		{
			Log.Warning($"{Where()}: {obj.Name} no longer has a {type.Name}, so it reads as null.");
		}
		return found;
	}

	private Obj FindObj(string key, string what)
	{
		var found = SpawnedKeys.ForLookup(key);
		var obj = found == null ? Obj.None : NativeSaves.FindObj(found);
		if (!obj.IsValid && found == null)
		{
			Log.Warning($"{Where()}: the save names {what} that a script made while the game ran, which only a save of the scene makes again, so it reads as nothing.");
		}
		else if (!obj.IsValid)
		{
			Log.Warning($"{Where()}: the save names {what} in a scene that is not open now, so it reads as nothing. Read the save once its level is open.");
		}
		return obj;
	}

	private object? Structured(SaveNode node, Type type, object? into)
	{
		if (Unsupported.Why(type) is { } why)
		{
			throw new SaveException(SaveError.Unsupported, $"{Where()}: {why}.");
		}
		if (Dictionaries.Element(type) is { } pair)
		{
			return Dictionary(node, type, pair.Key, pair.Value);
		}
		if (Collections.Element(type) is { } element)
		{
			return List(node, type, element);
		}
		return Members(node, type, into);
	}

	private object List(SaveNode node, Type type, Type element)
	{
		if (node.Kind != SaveNodeKind.List)
		{
			throw new FormatException($"The save has {node.Shown()} where a list was expected.");
		}

		var items = new List<object?>(node.Count);
		for (var at = 0; at < node.Count; at++)
		{
			path.Add($"[{at}]");
			items.Add(TryValue(node[at], element, out var item) ? item : Default(element));
			path.RemoveAt(path.Count - 1);
		}
		return Collections.Make(type, element, items);
	}

	private object Dictionary(SaveNode node, Type type, Type key, Type value)
	{
		var entries = new List<(object Key, object? Value)>();
		if (node.Kind == SaveNodeKind.Table)
		{
			foreach (var field in node.Fields)
			{
				path.Add($"[{field.Key}]");
				if (TryKey(field.Key, key, out var entryKey) && TryValue(field.Value, value, out var entry))
				{
					entries.Add((entryKey, entry));
				}
				path.RemoveAt(path.Count - 1);
			}
		}
		else if (node.Kind == SaveNodeKind.List)
		{
			foreach (var pair in node.Items)
			{
				if (pair.Kind == SaveNodeKind.List && pair.Count == 2 && TryValue(pair[0], key, out var entryKey) && entryKey != null
					&& TryValue(pair[1], value, out var entryValue))
				{
					entries.Add((entryKey, entryValue));
				}
			}
		}
		else
		{
			throw new FormatException($"The save has {node.Shown()} where a dictionary was expected.");
		}
		return Dictionaries.Make(type, key, value, entries);
	}

	private object Members(SaveNode node, Type type, object? into)
	{
		if (node.Kind != SaveNodeKind.Table)
		{
			throw new FormatException($"The save has {node.Shown()} where a {type.Name} was expected.");
		}

		Migrate(node, type);
		var target = into ?? Make(type);
		if (node.Has("$id") && !type.IsValueType)
		{
			made[node["$id"].AsLong()] = target;
		}
		Fill(node, target, SaveMembers.Of(type));
		return target;
	}

	private void Fill(SaveNode node, object target, IReadOnlyList<SaveMember> members)
	{
		foreach (var member in members)
		{
			var value = Field(node, member.Name);
			if (value == null)
			{
				continue;
			}

			path.Add("." + member.Name);
			if (TryValue(value, member.Type, out var read))
			{
				Set(member, target, read);
			}
			path.RemoveAt(path.Count - 1);
		}
	}

	/// <summary>Brings a type's table up to date, one version at a time, and marks it so, so it is never migrated twice.</summary>
	private void Migrate(SaveNode node, Type type)
	{
		var (version, _) = SaveTypes.Version(type);
		var saved = node.Has("$version") ? node["$version"].AsInt() : 1;
		if (saved > version)
		{
			throw new SaveException(SaveError.NewerVersion,
				$"{Where()}: a newer version of the game wrote it, with {type.Name} at version {saved}, and this one reads {type.Name} up to version {version}.");
		}
		if (saved == version)
		{
			return;
		}

		var steps = SaveTypes.Migrations(type);
		for (; saved < version; saved++)
		{
			if (steps.TryGetValue(saved, out var migrate))
			{
				Run(migrate, node, type, saved);
			}
		}
		node["$version"] = version;
	}

	private void Run(Action<SaveNode> migrate, SaveNode node, Type type, int from)
	{
		try
		{
			migrate(node);
		}
		catch (Exception exception) when (exception is not SaveException)
		{
			throw new SaveException(SaveError.Unsupported, $"{Where()}: migrating {type.Name} from version {from} threw: {exception.Message}", exception);
		}
	}

	/// <summary>The value saved under a member's name, or under one that differs only in case and underscores.</summary>
	private static SaveNode? Field(SaveNode node, string name)
	{
		if (node.Has(name))
		{
			return node[name];
		}

		var wanted = BehaviourFields.Normalize(name);
		foreach (var field in node.Fields)
		{
			if (!field.Key.StartsWith('$') && BehaviourFields.Normalize(field.Key) == wanted)
			{
				return field.Value;
			}
		}
		return null;
	}

	private object Make(Type type)
	{
		if (typeof(Behaviour).IsAssignableFrom(type))
		{
			throw new SaveException(SaveError.Unsupported,
				$"{Where()}: a save cannot make a {type.Name}, as only an Obj can have a behaviour. Read the save into the behaviour with Save.ReadInto.");
		}
		if (type.IsAbstract || type.IsInterface)
		{
			throw new FormatException($"The save does not say which kind of {type.Name} it holds.");
		}
		if (type.IsValueType)
		{
			return Activator.CreateInstance(type)!;
		}

		var constructor = type.GetConstructor(BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic, Type.EmptyTypes);
		return constructor != null ? constructor.Invoke(null) : RuntimeHelpers.GetUninitializedObject(type);
	}

	private void Set(SaveMember member, object target, object? value)
	{
		try
		{
			member.Set(target, value);
		}
		catch (Exception exception) when (exception is ArgumentException or TargetInvocationException)
		{
			Warn(exception.InnerException ?? exception);
		}
	}

	/// <summary>Reads a value, or warns and gives false when it does not fit. A newer version still stops the whole read.</summary>
	private bool TryValue(SaveNode node, Type type, out object? value)
	{
		try
		{
			value = Value(node, type, null);
			return true;
		}
		catch (Exception exception) when (IsBadValue(exception))
		{
			Warn(exception);
			value = null;
			return false;
		}
	}

	private bool TryKey(string text, Type type, out object key)
	{
		try
		{
			key = Dictionaries.ParseKey(text, type);
			return true;
		}
		catch (Exception exception) when (IsBadValue(exception))
		{
			Warn(new FormatException($"The key {text} is not a {type.Name}."));
			key = text;
			return false;
		}
	}

	private static bool IsBadValue(Exception exception)
	{
		return exception is SaveException { Error: SaveError.Unsupported } || BehaviourFields.IsBadValue(exception)
			|| exception is InvalidCastException or KeyNotFoundException or TargetInvocationException;
	}

	private void Warn(Exception exception)
	{
		var message = exception is SaveException ? exception.Message : $"{Where()}: {exception.Message}";
		Log.Warning($"{message} It keeps the value it starts with.");
	}

	private static object? Default(Type type)
	{
		return type.IsValueType ? Activator.CreateInstance(type) : null;
	}

	private string Where()
	{
		var where = string.Concat(path).TrimStart('.');
		var about = context.Length > 0 ? $"Reading \"{context}\"" : "Reading a save";
		return where.Length > 0 ? $"{about}, at {where}" : about;
	}
}
