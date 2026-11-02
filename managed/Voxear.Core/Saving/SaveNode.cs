using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using Voxear.Saving;

namespace Voxear;

/// <summary>
/// A value in a save, after a script's values are taken apart and before they are turned into bytes: a table of
/// named values, a list, text, a number, true or false, raw bytes, or nothing. Every format writes these and reads
/// them back, so a migration or a converter works the same whichever format a save is in. Reading a name a table
/// does not have gives a Null node rather than throwing, so a migration can look before it changes anything.
/// </summary>
public sealed class SaveNode
{
	/// <summary>The node for nothing. It cannot be changed, so every missing value can share it.</summary>
	public static readonly SaveNode Null = new SaveNode(SaveNodeKind.Null, null);

	private readonly object? value;
	private readonly List<KeyValuePair<string, SaveNode>>? fields;
	private readonly List<SaveNode>? items;

	private SaveNode(SaveNodeKind kind, object? value)
	{
		Kind = kind;
		this.value = value;
		if (kind == SaveNodeKind.Table)
		{
			fields = new List<KeyValuePair<string, SaveNode>>();
		}
		else if (kind == SaveNodeKind.List)
		{
			items = new List<SaveNode>();
		}
	}

	public SaveNodeKind Kind { get; }

	public bool IsNull => Kind == SaveNodeKind.Null;

	/// <summary>How many values a table or list holds; 0 for anything else.</summary>
	public int Count => fields?.Count ?? items?.Count ?? 0;

	/// <summary>A table's names, in order.</summary>
	public IEnumerable<string> Names => Fields.Select(entry => entry.Key);

	/// <summary>A table's names and values, in order.</summary>
	public IReadOnlyList<KeyValuePair<string, SaveNode>> Fields => fields ?? new List<KeyValuePair<string, SaveNode>>();

	/// <summary>A list's values, in order.</summary>
	public IReadOnlyList<SaveNode> Items => items ?? new List<SaveNode>();

	/// <summary>A table's value by name, or Null when it has none. Setting adds the name, or replaces its value in place.</summary>
	public SaveNode this[string name]
	{
		get
		{
			var at = IndexOf(name);
			return at < 0 ? Null : fields![at].Value;
		}
		set
		{
			var table = Table("set a value by name");
			var at = IndexOf(name);
			var entry = new KeyValuePair<string, SaveNode>(name, value ?? Null);
			if (at < 0)
			{
				table.Add(entry);
			}
			else
			{
				table[at] = entry;
			}
		}
	}

	/// <summary>A list's value by place.</summary>
	public SaveNode this[int index]
	{
		get { return List("read a value by index")[index]; }
		set { List("set a value by index")[index] = value ?? Null; }
	}

	/// <summary>An empty table, to fill by name.</summary>
	public static SaveNode NewTable()
	{
		return new SaveNode(SaveNodeKind.Table, null);
	}

	/// <summary>A list of these values.</summary>
	public static SaveNode NewList(params IEnumerable<SaveNode> values)
	{
		var list = new SaveNode(SaveNodeKind.List, null);
		list.items!.AddRange(values);
		return list;
	}

	public static SaveNode Bytes(byte[] bytes)
	{
		return new SaveNode(SaveNodeKind.Bytes, bytes);
	}

	/// <summary>
	/// Takes a value apart as a save does, with the same converters. A converter calls it for the values inside
	/// the one it converts.
	/// </summary>
	public static SaveNode From<T>(T value)
	{
		return SaveWriter.Write(value, typeof(T));
	}

	public static implicit operator SaveNode(bool value)
	{
		return new SaveNode(SaveNodeKind.Bool, value);
	}

	public static implicit operator SaveNode(int value)
	{
		return new SaveNode(SaveNodeKind.Integer, (long)value);
	}

	public static implicit operator SaveNode(long value)
	{
		return new SaveNode(SaveNodeKind.Integer, value);
	}

	public static implicit operator SaveNode(float value)
	{
		return new SaveNode(SaveNodeKind.Float, (double)value);
	}

	public static implicit operator SaveNode(double value)
	{
		return new SaveNode(SaveNodeKind.Float, value);
	}

	public static implicit operator SaveNode(string? value)
	{
		return value == null ? Null : new SaveNode(SaveNodeKind.Text, value);
	}

	/// <summary>Whether a table has a value by this name.</summary>
	public bool Has(string name)
	{
		return IndexOf(name) >= 0;
	}

	/// <summary>Takes a value out of a table. False when it had none by that name.</summary>
	public bool Remove(string name)
	{
		var at = IndexOf(name);
		if (at >= 0)
		{
			fields!.RemoveAt(at);
		}
		return at >= 0;
	}

	/// <summary>Gives a table's value another name, keeping its place, and replacing any value already called that. False when it had none called from.</summary>
	public bool Rename(string from, string to)
	{
		var at = IndexOf(from);
		if (at < 0 || from == to)
		{
			return at >= 0;
		}

		var moved = fields![at].Value;
		Remove(to);
		fields[IndexOf(from)] = new KeyValuePair<string, SaveNode>(to, moved);
		return true;
	}

	/// <summary>Adds a value to the end of a list.</summary>
	public void Add(SaveNode item)
	{
		List("add a value").Add(item ?? Null);
	}

	public bool AsBool()
	{
		return value switch
		{
			bool on => on,
			long number => number != 0,
			_ => throw Expected("true or false"),
		};
	}

	public long AsLong()
	{
		return value switch
		{
			long number => number,
			double number when number == Math.Floor(number) => checked((long)number),
			string text when long.TryParse(text, NumberStyles.Integer, CultureInfo.InvariantCulture, out var parsed) => parsed,
			_ => throw Expected("a whole number"),
		};
	}

	public int AsInt()
	{
		return checked((int)AsLong());
	}

	public double AsDouble()
	{
		return value switch
		{
			double number => number,
			long number => number,
			string text when double.TryParse(text, NumberStyles.Float, CultureInfo.InvariantCulture, out var parsed) => parsed,
			_ => throw Expected("a number"),
		};
	}

	public float AsFloat()
	{
		return (float)AsDouble();
	}

	/// <summary>Text as it is; a number or true or false as text; null for Null.</summary>
	public string? AsString()
	{
		return value switch
		{
			null when Kind == SaveNodeKind.Null => null,
			string text => text,
			long number => number.ToString(CultureInfo.InvariantCulture),
			double number => number.ToString("R", CultureInfo.InvariantCulture),
			bool on => on ? "true" : "false",
			_ => throw Expected("text"),
		};
	}

	/// <summary>Raw bytes, or base64 text as JSON writes them.</summary>
	public byte[] AsBytes()
	{
		return value switch
		{
			byte[] bytes => bytes,
			string text => Convert.FromBase64String(text),
			_ => throw Expected("bytes"),
		};
	}

	/// <summary>Reads the node as a T, as a save is read back, migrations and converters included.</summary>
	public T As<T>()
	{
		return (T)SaveReader.Read(this, typeof(T), null)!;
	}

	/// <summary>The node as JSON, to look at while writing a migration.</summary>
	public override string ToString()
	{
		return System.Text.Encoding.UTF8.GetString(SaveFormat.Json.Write(this));
	}

	/// <summary>The value a primitive node holds, for the formats: bool, long, double, string or byte[].</summary>
	internal object? Value => value;

	internal static SaveNode Integer(long number)
	{
		return new SaveNode(SaveNodeKind.Integer, number);
	}

	internal static SaveNode Text(string text)
	{
		return new SaveNode(SaveNodeKind.Text, text);
	}

	/// <summary>Adds a name without looking for it first, as a format reading a table does.</summary>
	internal void AddField(string name, SaveNode node)
	{
		Table("add a value by name").Add(new KeyValuePair<string, SaveNode>(name, node));
	}

	/// <summary>Puts a name first, as the reference marks a table only learns it needs once it is written.</summary>
	internal void Prepend(string name, SaveNode node)
	{
		Table("add a value by name").Insert(0, new KeyValuePair<string, SaveNode>(name, node));
	}

	private int IndexOf(string name)
	{
		if (fields == null)
		{
			return -1;
		}

		for (var at = 0; at < fields.Count; at++)
		{
			if (fields[at].Key == name)
			{
				return at;
			}
		}
		return -1;
	}

	private List<KeyValuePair<string, SaveNode>> Table(string doing)
	{
		return fields ?? throw new InvalidOperationException($"Only a table can {doing}; this is {Shown()}.");
	}

	private List<SaveNode> List(string doing)
	{
		return items ?? throw new InvalidOperationException($"Only a list can {doing}; this is {Shown()}.");
	}

	private FormatException Expected(string what)
	{
		return new FormatException($"The save has {Shown()} where {what} was expected.");
	}

	/// <summary>What the node is, in words, for messages.</summary>
	internal string Shown()
	{
		return Kind switch
		{
			SaveNodeKind.Null => "nothing",
			SaveNodeKind.Bool => (bool)value! ? "true" : "false",
			SaveNodeKind.Integer or SaveNodeKind.Float => $"the number {AsString()}",
			SaveNodeKind.Text => $"the text \"{value}\"",
			SaveNodeKind.Bytes => "bytes",
			SaveNodeKind.List => $"a list of {Count}",
			_ => "a table",
		};
	}
}
