using System;
using System.Collections.Immutable;
using System.IO;
using System.Linq;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;

namespace Voxear.Analyzers;

/// <summary>
/// Makes Actions from the project's input actions, so a script names an action as Actions.Player.Jump, an
/// InputActionRef by the action's id, rather than as "Jump" in quotes: renaming the action keeps the code working,
/// and taking it away is an error where it is used. Game scripts get their .inputactions as additional files through
/// Voxear.Scripts.props; a project without one plays with Voxear's own actions, and gets those. Actions goes in their
/// RootNamespace.
/// </summary>
[Generator(LanguageNames.CSharp)]
public sealed class InputActionsGenerator : IIncrementalGenerator
{
	public void Initialize(IncrementalGeneratorInitializationContext context)
	{
		var files = context.AdditionalTextsProvider
			.Where(file => file.Path.EndsWith(".inputactions", StringComparison.OrdinalIgnoreCase))
			.Select((file, token) => (file.Path, Text: file.GetText(token)?.ToString() ?? string.Empty));
		var rootNamespace = context.AnalyzerConfigOptionsProvider.Select((options, _) =>
			options.GlobalOptions.TryGetValue("build_property.RootNamespace", out var name) ? name.Trim() : string.Empty);
		context.RegisterSourceOutput(files.Collect().Combine(rootNamespace), Generate);
	}

	private static void Generate(SourceProductionContext context, (ImmutableArray<(string Path, string Text)> Files, string RootNamespace) input)
	{
		// The game reads the first file by path, as the engine does.
		var text = input.Files.OrderBy(file => file.Path, StringComparer.Ordinal).Select(file => file.Text).FirstOrDefault() ?? Defaults();
		var actions = InputActionsReader.Read(text);
		if (actions.Count == 0)
		{
			return;
		}
		var rootNamespace = IsNamespace(input.RootNamespace) ? input.RootNamespace : string.Empty;
		context.AddSource("Actions.g.cs", InputActionsSource.Write(actions, rootNamespace));
	}

	/// <summary>Voxear's own input actions, as the engine plays them when a project has none of its own.</summary>
	private static string Defaults()
	{
		using var stream = typeof(InputActionsGenerator).Assembly.GetManifestResourceStream("defaults.inputactions");
		if (stream == null)
		{
			return string.Empty;
		}
		using var reader = new StreamReader(stream);
		return reader.ReadToEnd();
	}

	private static bool IsNamespace(string name)
	{
		return name.Split('.').All(part => SyntaxFacts.IsValidIdentifier(part) && SyntaxFacts.GetKeywordKind(part) == SyntaxKind.None);
	}
}
