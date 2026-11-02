using System;
using System.Collections;
using System.Collections.Generic;
using System.Globalization;
using System.Reflection;
using Voxear.Interop;
using Voxear.Scripting;

namespace Voxear.Saving;

/// <summary>
/// Takes values apart into SaveNodes. Classes and structs become tables of their members (see SaveMembers), and
/// lists and arrays lists. A dictionary is a table when its keys read as text, and a list of [key, value] pairs when
/// they do not. An Obj, a behaviour another value points to, an asset such as a prefab or a sound, or a
/// Ref&lt;T&gt; is kept as what finds it again in a later run: the Obj's scene and place, or the asset's GUID. A value whose type is not the one its field
/// declares records "$type", and an object reached twice is written once, marked "$id", and then as "$ref", so
/// shared and circular references read back as they were. A behaviour at the top of a save is its fields, and a
/// SaveNode is kept as it is.
/// </summary>
internal sealed class SaveWriter
{
	[ThreadStatic]
	private static SaveWriter? current;

	private readonly Dictionary<object, SaveNode> written = new Dictionary<object, SaveNode>(ReferenceEqualityComparer.Instance);
	private readonly Dictionary<object, int> ids = new Dictionary<object, int>(ReferenceEqualityComparer.Instance);
	private readonly HashSet<object> open = new HashSet<object>(ReferenceEqualityComparer.Instance);
	private readonly List<string> path = new List<string>();
	private readonly string context;

	private SaveWriter(string context)
	{
		this.context = context;
	}

	/// <summary>Takes a value apart. Inside a converter, the save being written carries on, sharing its references.</summary>
	public static SaveNode Write(object? value, Type declared, string context = "")
	{
		if (current != null)
		{
			return current.Value(value, declared, false);
		}

		using var scope = Begin(context);
		return current!.Value(value, declared, true);
	}

	/// <summary>A table of only these members of target, as a save of the scene keeps a behaviour's marked ones.</summary>
	public static SaveNode WriteMembers(object target, IReadOnlyList<SaveMember> members, string context = "")
	{
		using var scope = Begin(context);
		return current!.Table(target, members, false);
	}

	/// <summary>One save written in several calls, sharing its references, until the scope ends.</summary>
	public static IDisposable Begin(string context)
	{
		if (current != null)
		{
			return new Scope(null);
		}
		current = new SaveWriter(context);
		return new Scope(() => current = null);
	}

	private SaveNode Value(object? value, Type declared, bool top)
	{
		if (value == null || value is SaveNode)
		{
			return value as SaveNode ?? SaveNode.Null;
		}

		var type = value.GetType();
		var expected = Nullable.GetUnderlyingType(declared) ?? declared;
		var node = Converted(value, type) ?? Plain(value, type) ?? Found(value, type, top);
		if (node == null)
		{
			Refuse(type);
			node = Collection(value, type);
		}
		if (node != null)
		{
			return type == expected ? node : Wrapped(node, type);
		}

		var table = Members(value, type);
		if (type != expected && !table.Has("$ref"))
		{
			table.Prepend("$type", SaveTypes.NameOf(type));
		}
		return table;
	}

	/// <summary>A value whose type is not the declared one, and that is not a table of members to put "$type" in.</summary>
	private static SaveNode Wrapped(SaveNode node, Type type)
	{
		var wrapped = SaveNode.NewTable();
		wrapped["$type"] = SaveTypes.NameOf(type);
		wrapped["$value"] = node;
		return wrapped;
	}

	private SaveNode? Converted(object value, Type type)
	{
		var converter = SaveTypes.Converter(type);
		if (converter == null)
		{
			return null;
		}

		try
		{
			return converter.WriteObject(value);
		}
		catch (Exception exception) when (exception is not SaveException)
		{
			throw Failure($"{converter.GetType().Name} threw: {exception.Message}", exception);
		}
	}

	private static SaveNode? Plain(object value, Type type)
	{
		return value switch
		{
			string text => text,
			bool on => on,
			Enum named => named.ToString(),
			float number => number,
			double number => number,
			decimal number => number.ToString(CultureInfo.InvariantCulture),
			char letter => letter.ToString(),
			ulong number => unchecked((long)number),
			byte[] bytes => SaveNode.Bytes(bytes),
			_ when FieldValues.IsInteger(type) => Convert.ToInt64(value, CultureInfo.InvariantCulture),
			_ => null,
		};
	}

	/// <summary>The engine's things, kept as what finds them in a later run.</summary>
	private SaveNode? Found(object value, Type type, bool top)
	{
		if (value is Obj obj)
		{
			return ObjKey(obj, obj.Name);
		}
		if (value is Behaviour behaviour && !top)
		{
			return ObjKey(behaviour.Obj, $"{type.Name} on {behaviour.Obj.Name}");
		}
		if (FieldValues.IsAsset(type))
		{
			return AssetGuid(value, type);
		}
		if (FieldValues.IsGeneric(type, typeof(Ref<>)))
		{
			var reference = (string)type.GetProperty(nameof(Ref<Prefab>.Path))!.GetValue(value)!;
			return reference.Length == 0 ? SaveNode.Null : NativeSaves.Key(NativeSaves.KeyReference, 0, reference) ?? reference;
		}
		return null;
	}

	/// <summary>A loaded asset, such as a Prefab or a Sound, as the GUID every asset type gives through IAsset.</summary>
	private static SaveNode AssetGuid(object asset, Type type)
	{
		var contract = typeof(IAsset<>).MakeGenericType(type);
		var guid = contract.GetProperty(nameof(IAsset<Prefab>.AssetGuid), BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic)!;
		return (string?)guid.GetValue(asset);
	}

	private SaveNode ObjKey(Obj obj, string shown)
	{
		if (!obj.IsValid)
		{
			return SaveNode.Null;
		}

		var key = SpawnedKeys.ForSave(NativeSaves.Key(NativeSaves.KeyObj, obj.Id, string.Empty));
		if (key == null)
		{
			Log.Warning($"{Where()}: {shown} was made while the game ran, so a save cannot find it again, and keeps nothing for it. A save of the scene makes such Objs again.");
		}
		return key;
	}

	private SaveNode? Collection(object value, Type type)
	{
		if (Dictionaries.Element(type) is { } pair)
		{
			return Inside(value, () => Dictionary((IEnumerable)value, pair.Key, pair.Value));
		}
		if (Collections.Element(type) is { } element)
		{
			return Inside(value, () => List((IEnumerable)value, element));
		}
		return null;
	}

	private void Refuse(Type type)
	{
		if (Unsupported.Why(type) is { } why)
		{
			throw Failure(why, null);
		}
	}

	/// <summary>A collection that holds itself would never end, so it is refused rather than followed round.</summary>
	private SaveNode Inside(object collection, Func<SaveNode> write)
	{
		if (!open.Add(collection))
		{
			throw Failure("it holds itself, and a save cannot keep a list or dictionary inside itself", null);
		}
		try
		{
			return write();
		}
		finally
		{
			open.Remove(collection);
		}
	}

	private SaveNode List(IEnumerable items, Type element)
	{
		var list = SaveNode.NewList();
		foreach (var item in items)
		{
			path.Add($"[{list.Count}]");
			list.Add(Value(item, element, false));
			path.RemoveAt(path.Count - 1);
		}
		return list;
	}

	private SaveNode Dictionary(IEnumerable entries, Type key, Type value)
	{
		var byText = Dictionaries.KeysAreText(key);
		var node = byText ? SaveNode.NewTable() : SaveNode.NewList();
		foreach (var (entryKey, entryValue) in Dictionaries.Entries(entries))
		{
			path.Add($"[{entryKey}]");
			if (byText)
			{
				node.AddField(Dictionaries.KeyText(entryKey), Value(entryValue, value, false));
			}
			else
			{
				node.Add(SaveNode.NewList(Value(entryKey, key, false), Value(entryValue, value, false)));
			}
			path.RemoveAt(path.Count - 1);
		}
		return node;
	}

	private SaveNode Members(object value, Type type)
	{
		if (!type.IsValueType && written.TryGetValue(value, out var first))
		{
			return Reference(value, first);
		}

		return Table(value, SaveMembers.Of(type), true);
	}

	private SaveNode Table(object value, IReadOnlyList<SaveMember> members, bool versioned)
	{
		var type = value.GetType();
		var table = SaveNode.NewTable();
		if (!type.IsValueType)
		{
			written[value] = table;
		}
		var (version, marked) = SaveTypes.Version(type);
		if (marked && versioned)
		{
			table["$version"] = version;
		}
		foreach (var member in members)
		{
			path.Add("." + member.Name);
			table.AddField(member.Name, Value(Read(member, value), member.Type, false));
			path.RemoveAt(path.Count - 1);
		}
		return table;
	}

	private object? Read(SaveMember member, object target)
	{
		try
		{
			return member.Get(target);
		}
		catch (TargetInvocationException exception)
		{
			throw Failure($"reading it threw: {exception.InnerException?.Message}", exception.InnerException);
		}
	}

	/// <summary>An object reached again: its first table gets an "$id", and this place a "$ref" to it.</summary>
	private SaveNode Reference(object value, SaveNode first)
	{
		if (!ids.TryGetValue(value, out var id))
		{
			ids[value] = id = ids.Count + 1;
			first.Prepend("$id", id);
		}

		var reference = SaveNode.NewTable();
		reference["$ref"] = id;
		return reference;
	}

	private string Where()
	{
		var where = string.Concat(path);
		var about = context.Length > 0 ? $"Saving \"{context}\"" : "Saving";
		return where.Length > 0 ? $"{about}, at {where.TrimStart('.')}" : about;
	}

	private SaveException Failure(string why, Exception? inner)
	{
		var message = $"{Where()}: {why}.";
		return inner == null ? new SaveException(SaveError.Unsupported, message) : new SaveException(SaveError.Unsupported, message, inner);
	}
}
