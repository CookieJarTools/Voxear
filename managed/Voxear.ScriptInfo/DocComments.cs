using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using System.Text.RegularExpressions;
using System.Xml.Linq;

namespace Voxear.ScriptInfo;

/// <summary>
/// The <c>///</c> comments of some assemblies, from the XML files the compiler writes beside them, turned into
/// the Markdown VoxDocs reads. A reference to a type the Code Reference has a page for becomes a link to it;
/// one to anything else is written as code.
/// </summary>
internal sealed class DocComments
{
	private static readonly Regex Spaces = new Regex(@"\s+");

	private readonly Dictionary<string, XElement> members = new Dictionary<string, XElement>();
	private readonly HashSet<string> pages;

	/// <param name="pages">The types with pages of their own, by their full names.</param>
	public DocComments(IEnumerable<string> files, HashSet<string> pages)
	{
		this.pages = pages;
		foreach (var file in files.Where(File.Exists))
		{
			Read(file);
		}
	}

	/// <summary>A part of a member's comment, <c>summary</c> or <c>remarks</c>, as Markdown, or nothing.</summary>
	public string Part(string id, string part)
	{
		var element = Element(id)?.Element(part);
		return element == null ? string.Empty : Markdown(element);
	}

	/// <summary>Whether a member has a comment of its own, rather than none or a bare <c>&lt;inheritdoc/&gt;</c>.</summary>
	public bool Has(string id)
	{
		return Element(id) != null;
	}

	/// <summary>What a member's comment says of each of its parameters, by name.</summary>
	public IEnumerable<(string name, string text)> Parameters(string id)
	{
		var element = Element(id);
		if (element == null)
		{
			return Enumerable.Empty<(string, string)>();
		}

		return element.Elements("param").Select(p => ((string?)p.Attribute("name") ?? string.Empty, Markdown(p)));
	}

	private void Read(string file)
	{
		var document = XDocument.Load(file);
		foreach (var member in document.Descendants("member"))
		{
			var name = (string?)member.Attribute("name");
			if (name != null)
			{
				members[name] = member;
			}
		}
	}

	/// <summary>
	/// A member's comment, or the one it inherits with <c>&lt;inheritdoc cref="..."/&gt;</c>. A bare
	/// <c>&lt;inheritdoc/&gt;</c> is no comment here: what it inherits is found from the member it overrides.
	/// </summary>
	private XElement? Element(string id)
	{
		if (!members.TryGetValue(id, out var element))
		{
			return null;
		}

		var inherit = element.Element("inheritdoc");
		if (inherit == null)
		{
			return element;
		}

		var cref = (string?)inherit.Attribute("cref");
		return cref != null && members.TryGetValue(cref, out var inherited) ? inherited : null;
	}

	private string Markdown(XElement element)
	{
		var text = new StringBuilder();
		foreach (var node in element.Nodes())
		{
			text.Append(Node(node));
		}

		var paragraphs = text.ToString().Split("\n\n").Select(p => p.StartsWith("```") ? p.Trim('\n') : Spaces.Replace(p, " ").Trim());
		return string.Join("\n\n", paragraphs.Where(p => p.Length > 0));
	}

	private string Node(XNode node)
	{
		if (node is XText text)
		{
			return text.Value;
		}

		if (node is not XElement element)
		{
			return string.Empty;
		}

		switch (element.Name.LocalName)
		{
			case "c":
				return "`" + element.Value + "`";
			case "code":
				return "\n\n```csharp\n" + Indented(element.Value) + "\n```\n\n";
			case "para":
				return "\n\n" + Markdown(element) + "\n\n";
			case "see":
			case "seealso":
				return See(element);
			case "paramref":
			case "typeparamref":
				return "`" + (string?)element.Attribute("name") + "`";
			case "list":
				return "\n\n" + string.Join("\n", element.Elements("item").Select(i => "- " + Markdown(i))) + "\n\n";
			default:
				return Markdown(element);
		}
	}

	private string See(XElement element)
	{
		var word = (string?)element.Attribute("langword");
		if (word != null)
		{
			return "`" + word + "`";
		}

		var href = (string?)element.Attribute("href");
		if (href != null)
		{
			return "[" + (element.Value.Length > 0 ? element.Value : href) + "](" + href + ")";
		}

		var cref = (string?)element.Attribute("cref") ?? string.Empty;
		var (type, shown) = Target(cref);
		var words = element.Value.Length > 0 ? element.Value : shown;
		return pages.Contains(type) ? "[[code:" + type + "|" + words + "]]" : "`" + words + "`";
	}

	/// <summary>
	/// The type a <c>cref</c> is on, and how to write it: <c>T:Voxear.Prefab</c> is Prefab, and
	/// <c>M:Voxear.Prefab.Load(System.String)</c> is Prefab.Load, on Prefab's page.
	/// </summary>
	private static (string type, string shown) Target(string cref)
	{
		var name = cref.Length > 2 && cref[1] == ':' ? cref.Substring(2) : cref;
		var bracket = name.IndexOf('(');
		if (bracket >= 0)
		{
			name = name.Substring(0, bracket);
		}

		if (cref.StartsWith("T:"))
		{
			return (name, Short(name));
		}

		var dot = name.LastIndexOf('.');
		var type = dot < 0 ? name : name.Substring(0, dot);
		return (type, Short(type) + "." + name.Substring(dot + 1));
	}

	/// <summary>A full type name's last part, without its generic arity: <c>Voxear.Ref`1</c> is Ref.</summary>
	private static string Short(string name)
	{
		var last = name.Substring(name.LastIndexOf('.') + 1);
		var tick = last.IndexOf('`');
		return tick < 0 ? last : last.Substring(0, tick);
	}

	/// <summary>A code example without the indentation its comment gave every line.</summary>
	private static string Indented(string code)
	{
		var lines = code.Trim('\n', '\r').Split('\n');
		var indent = lines.Where(l => l.Trim().Length > 0).Select(l => l.Length - l.TrimStart().Length).DefaultIfEmpty(0).Min();
		return string.Join("\n", lines.Select(l => l.Length >= indent ? l.Substring(indent).TrimEnd() : l.Trim()));
	}
}
