using System;
using System.Collections.Immutable;
using System.Linq;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.Text;

namespace Voxear.Analyzers;

/// <summary>
/// Makes Strings from the project's string tables, so a script names a key as Strings.Menu.Play rather than as
/// "menu.play" in quotes, and a key that is missing or renamed is an error where it is used. Game scripts get their
/// tables as additional files through Voxear.Scripts.props, and Strings goes in their RootNamespace.
/// </summary>
[Generator(LanguageNames.CSharp)]
public sealed class StringKeysGenerator : IIncrementalGenerator
{
	public void Initialize(IncrementalGeneratorInitializationContext context)
	{
		var tables = context.AdditionalTextsProvider
			.Where(file => file.Path.EndsWith(".strings", StringComparison.OrdinalIgnoreCase))
			.Select((file, token) => (file.Path, Text: file.GetText(token)?.ToString() ?? string.Empty));
		var rootNamespace = context.AnalyzerConfigOptionsProvider.Select((options, _) =>
			options.GlobalOptions.TryGetValue("build_property.RootNamespace", out var name) ? name.Trim() : string.Empty);
		context.RegisterSourceOutput(tables.Collect().Combine(rootNamespace), Generate);
	}

	private static void Generate(SourceProductionContext context, (ImmutableArray<(string Path, string Text)> Tables, string RootNamespace) input)
	{
		if (input.Tables.IsEmpty)
		{
			return;
		}
		var entries = input.Tables
			.OrderBy(table => table.Path, StringComparer.Ordinal)
			.SelectMany(table => StringTableReader.Read(table.Path, SourceText.From(table.Text)));
		var root = StringKeyTree.Build(entries, context.ReportDiagnostic);
		context.AddSource("Strings.g.cs", StringKeysSource.Write(root, IsNamespace(input.RootNamespace) ? input.RootNamespace : string.Empty));
	}

	/// <summary>Whether a project's RootNamespace can name a namespace; one that cannot leaves Strings in the global one.</summary>
	private static bool IsNamespace(string name)
	{
		return name.Split('.').All(part => SyntaxFacts.IsValidIdentifier(part) && SyntaxFacts.GetKeywordKind(part) == SyntaxKind.None);
	}
}
