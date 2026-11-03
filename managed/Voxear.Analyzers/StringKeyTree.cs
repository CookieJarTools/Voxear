using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;

namespace Voxear.Analyzers;

/// <summary>
/// Arranges the tables' keys as the generated Strings nests them: "menu.play" is Play in the class Menu. A key that
/// clashes with another, or would be named like the class it is in, is left out, with a warning at its table.
/// </summary>
internal static class StringKeyTree
{
	public const string RootName = "Strings";

	/// <summary>The keys as classes and members, a key in several tables once, as the first table has it.</summary>
	public static StringKeyNode Build(IEnumerable<StringTableEntry> entries, Action<Diagnostic> report)
	{
		var root = new StringKeyNode(RootName);
		var seen = new HashSet<string>(StringComparer.Ordinal);
		foreach (var entry in entries.Where(entry => seen.Add(entry.Key)))
		{
			Add(root, entry, report);
		}
		Prune(root, report);
		return root;
	}

	/// <summary>
	/// A key's part as a C# name: PascalCase, its words split on '-', '_', spaces and anything else a name cannot
	/// hold, and '_' first when it starts with a digit. Null when nothing is left.
	/// </summary>
	public static string? MemberName(string part)
	{
		var name = new StringBuilder();
		var wordStarts = true;
		foreach (var c in part)
		{
			if (c == '_' || !SyntaxFacts.IsIdentifierPartCharacter(c))
			{
				wordStarts = true;
				continue;
			}
			name.Append(wordStarts ? char.ToUpperInvariant(c) : c);
			wordStarts = false;
		}
		if (name.Length > 0 && !SyntaxFacts.IsIdentifierStartCharacter(name[0]))
		{
			name.Insert(0, '_');
		}
		return name.Length == 0 ? null : name.ToString();
	}

	private static void Add(StringKeyNode root, StringTableEntry entry, Action<Diagnostic> report)
	{
		var names = entry.Key.Split('.').Select(MemberName).ToList();
		if (names.Contains(null))
		{
			report(LeftOut(entry, "as a part of it makes no name in C#"));
			return;
		}
		var node = root;
		foreach (var name in names)
		{
			node = node.Child(name!);
		}
		node.Keys.Add(entry);
	}

	private static void Prune(StringKeyNode node, Action<Diagnostic> report)
	{
		foreach (var child in node.Children.Values.ToList())
		{
			if (child.Name == node.Name)
			{
				LeaveOut(child.AllKeys(), $"as {child.Name} would be named like the class it is in", report);
				node.Children.Remove(child.Name);
				continue;
			}
			Prune(child, report);
			PruneKeys(child, report);
			if (!child.IsGroup && child.Keys.Count == 0)
			{
				node.Children.Remove(child.Name);
			}
		}
	}

	/// <summary>Leaves out a key that is a group's name too, and keys that make the same name.</summary>
	private static void PruneKeys(StringKeyNode node, Action<Diagnostic> report)
	{
		if (node.IsGroup && node.Keys.Count > 0)
		{
			var inside = node.Children.Values.SelectMany(child => child.AllKeys()).First();
			LeaveOut(node.Keys, $"as \"{inside.Key}\" makes {node.Name} a group of keys", report);
			node.Keys.Clear();
		}
		if (node.Keys.Count > 1)
		{
			var keys = node.Keys.ToList();
			node.Keys.Clear();
			keys.ForEach(entry => report(LeftOut(entry, $"as \"{keys.First(other => other != entry).Key}\" makes the same name, {node.Name}")));
		}
	}

	private static void LeaveOut(IEnumerable<StringTableEntry> entries, string reason, Action<Diagnostic> report)
	{
		foreach (var entry in entries.ToList())
		{
			report(LeftOut(entry, reason));
		}
	}

	private static Diagnostic LeftOut(StringTableEntry entry, string reason)
	{
		return Diagnostic.Create(Diagnostics.LeftOutKey, entry.Location, entry.Key, reason);
	}
}
