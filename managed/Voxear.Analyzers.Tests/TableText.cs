using System.Threading;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.Text;

namespace Voxear.Analyzers.Tests;

/// <summary>A string table held in memory, as the compiler hands a project's additional files to a generator.</summary>
internal sealed class TableText : AdditionalText
{
	private readonly string text;

	public TableText(string path, string text)
	{
		Path = path;
		this.text = text;
	}

	public override string Path { get; }

	public override SourceText GetText(CancellationToken cancellationToken = default)
	{
		return SourceText.From(text);
	}
}
