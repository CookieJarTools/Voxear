using System;
using System.Collections.Generic;
using System.Linq;

namespace Voxear.Analyzers;

/// <summary>A class or a member of the generated Strings: a group of keys, a key, or both while they clash.</summary>
internal sealed class StringKeyNode
{
	public StringKeyNode(string name)
	{
		Name = name;
	}

	/// <summary>Its name in C#, such as Menu or Play.</summary>
	public string Name { get; }

	/// <summary>The groups and keys inside it, by their names.</summary>
	public SortedDictionary<string, StringKeyNode> Children { get; } = new(StringComparer.Ordinal);

	/// <summary>The keys that end here: one for a member, more when keys clash.</summary>
	public List<StringTableEntry> Keys { get; } = [];

	public bool IsGroup
	{
		get { return Children.Count > 0; }
	}

	/// <summary>The group or key inside it of this name, added when there is none.</summary>
	public StringKeyNode Child(string name)
	{
		if (!Children.TryGetValue(name, out var child))
		{
			child = new StringKeyNode(name);
			Children.Add(name, child);
		}
		return child;
	}

	/// <summary>Its keys and every key inside it.</summary>
	public IEnumerable<StringTableEntry> AllKeys()
	{
		return Keys.Concat(Children.Values.SelectMany(child => child.AllKeys()));
	}
}
