using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.Text;

namespace Voxear.Analyzers;

/// <summary>One key of a string table, as the generator needs it: its text in the table's first language and its comment.</summary>
internal sealed class StringTableEntry
{
	public StringTableEntry(string path, string key, TextSpan span, LinePositionSpan lines)
	{
		Path = path;
		Key = key;
		Span = span;
		Lines = lines;
	}

	/// <summary>The table's file.</summary>
	public string Path { get; }

	public string Key { get; }

	/// <summary>What a translator should know about it; empty for nothing.</summary>
	public string Comment { get; set; } = string.Empty;

	/// <summary>Its text in the table's first language; empty for none.</summary>
	public string Text { get; set; } = string.Empty;

	/// <summary>Where its key is written in the table.</summary>
	public TextSpan Span { get; }

	public LinePositionSpan Lines { get; }

	public Location Location
	{
		get { return Location.Create(Path, Span, Lines); }
	}
}
