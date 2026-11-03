using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text;
using Microsoft.CodeAnalysis.Text;

namespace Voxear.Analyzers;

/// <summary>
/// Reads the keys of a .strings table, which is TOML, as far as Voxear writes it: [[entry]] tables of basic and literal
/// strings. What it cannot read, such as a number or a string over several lines, it passes over rather than failing.
/// </summary>
internal sealed class StringTableReader
{
	private readonly string path;
	private readonly SourceText text;
	private readonly List<StringTableEntry> entries = [];
	private readonly List<(string Language, string Text)> texts = [];
	private string? firstLanguage;
	private bool inEntry;
	private string? key;
	private TextLine keyLine;
	private string comment = string.Empty;

	/// <summary>The end of a string over several lines that is being passed over, or null outside one.</summary>
	private string? multiLineEnd;

	private StringTableReader(string path, SourceText text)
	{
		this.path = path;
		this.text = text;
	}

	/// <summary>The table's entries in its file's order, each with a key.</summary>
	public static List<StringTableEntry> Read(string path, SourceText text)
	{
		var reader = new StringTableReader(path, text);
		foreach (var line in text.Lines)
		{
			reader.ReadLine(line);
		}
		reader.EndEntry();
		return reader.entries;
	}

	private void ReadLine(TextLine line)
	{
		var content = line.ToString();
		if (multiLineEnd != null)
		{
			multiLineEnd = content.Contains(multiLineEnd) ? null : multiLineEnd;
			return;
		}
		var trimmed = content.Trim();
		if (trimmed.StartsWith("[", StringComparison.Ordinal))
		{
			EndEntry();
			var end = trimmed.IndexOf("]]", StringComparison.Ordinal);
			inEntry = trimmed.StartsWith("[[", StringComparison.Ordinal) && end > 2 && trimmed.Substring(2, end - 2).Trim() == "entry";
			return;
		}
		if (trimmed.Length > 0 && trimmed[0] != '#')
		{
			ReadValue(line, content);
		}
	}

	private void ReadValue(TextLine line, string content)
	{
		var at = 0;
		var name = ReadKey(content, ref at);
		SkipSpaces(content, ref at);
		if (name == null || at >= content.Length || content[at] != '=')
		{
			return;
		}
		at++;
		SkipSpaces(content, ref at);
		var value = ReadString(content.Substring(at));
		if (value != null && inEntry)
		{
			Assign(name, value, line);
		}
	}

	private void Assign(string name, string value, TextLine line)
	{
		switch (name)
		{
			case "key":
				key = value.Trim();
				keyLine = line;
				break;
			case "comment":
				comment = value;
				break;
			case "limit":
				break;
			default:
				texts.Add((name, value));
				firstLanguage ??= name;
				break;
		}
	}

	private void EndEntry()
	{
		if (inEntry && !string.IsNullOrEmpty(key))
		{
			var lines = text.Lines.GetLinePositionSpan(keyLine.Span);
			var found = texts.Find(pair => pair.Language == firstLanguage).Text;
			entries.Add(new StringTableEntry(path, key!, keyLine.Span, lines) { Comment = comment, Text = found ?? string.Empty });
		}
		key = null;
		comment = string.Empty;
		texts.Clear();
	}

	/// <summary>A string value, or null for anything else; a string over several lines starts being passed over.</summary>
	private string? ReadString(string value)
	{
		if (value.StartsWith("\"\"\"", StringComparison.Ordinal) || value.StartsWith("'''", StringComparison.Ordinal))
		{
			var delimiter = value.Substring(0, 3);
			multiLineEnd = value.IndexOf(delimiter, 3, StringComparison.Ordinal) < 0 ? delimiter : null;
			return null;
		}
		if (value.StartsWith("'", StringComparison.Ordinal))
		{
			var end = value.IndexOf('\'', 1);
			return end < 0 ? null : value.Substring(1, end - 1);
		}
		return value.StartsWith("\"", StringComparison.Ordinal) ? Basic(value, 1, out _) : null;
	}

	/// <summary>A bare, "basic" or 'literal' key; null for one it cannot read, such as a dotted key.</summary>
	private static string? ReadKey(string content, ref int at)
	{
		SkipSpaces(content, ref at);
		if (at < content.Length && content[at] == '"')
		{
			var name = Basic(content, at + 1, out var end);
			at = end;
			return name;
		}
		if (at < content.Length && content[at] == '\'')
		{
			var end = content.IndexOf('\'', at + 1);
			var name = end < 0 ? null : content.Substring(at + 1, end - at - 1);
			at = end + 1;
			return name;
		}
		var start = at;
		while (at < content.Length && IsBare(content[at]))
		{
			at++;
		}
		return at > start ? content.Substring(start, at - start) : null;
	}

	/// <summary>A basic string from just after its opening quote, its escapes read; null when it does not end on the line.</summary>
	private static string? Basic(string content, int start, out int end)
	{
		var built = new StringBuilder();
		for (end = start; end < content.Length; end++)
		{
			var c = content[end];
			if (c == '"')
			{
				end++;
				return built.ToString();
			}
			if (c != '\\')
			{
				built.Append(c);
				continue;
			}
			end++;
			if (!Escape(content, ref end, built))
			{
				return null;
			}
		}
		return null;
	}

	/// <summary>Reads the escape whose letter is at <paramref name="at"/>, leaving it at the escape's last character.</summary>
	private static bool Escape(string content, ref int at, StringBuilder built)
	{
		if (at >= content.Length)
		{
			return false;
		}
		var simple = content[at] switch
		{
			'b' => "\b",
			't' => "\t",
			'n' => "\n",
			'f' => "\f",
			'r' => "\r",
			'e' => "\u001b",
			'"' => "\"",
			'\\' => "\\",
			_ => null,
		};
		if (simple != null)
		{
			built.Append(simple);
			return true;
		}
		return content[at] switch
		{
			'x' => Unicode(content, ref at, 2, built),
			'u' => Unicode(content, ref at, 4, built),
			'U' => Unicode(content, ref at, 8, built),
			_ => false,
		};
	}

	private static bool Unicode(string content, ref int at, int digits, StringBuilder built)
	{
		var hex = at + digits < content.Length ? content.Substring(at + 1, digits) : string.Empty;
		var read = int.TryParse(hex, NumberStyles.AllowHexSpecifier, CultureInfo.InvariantCulture, out var point);
		if (!read || point > 0x10FFFF || (point >= 0xD800 && point <= 0xDFFF))
		{
			return false;
		}
		built.Append(char.ConvertFromUtf32(point));
		at += digits;
		return true;
	}

	private static bool IsBare(char c)
	{
		return c is (>= 'a' and <= 'z') or (>= 'A' and <= 'Z') or (>= '0' and <= '9') or '-' or '_';
	}

	private static void SkipSpaces(string content, ref int at)
	{
		while (at < content.Length && (content[at] == ' ' || content[at] == '\t'))
		{
			at++;
		}
	}
}
