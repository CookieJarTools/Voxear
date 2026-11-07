using System.Collections.Immutable;
using System.Linq;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using Microsoft.CodeAnalysis.Diagnostics;

namespace Voxear.Analyzers;

/// <summary>
/// Says which module a name is in when a script uses one the compiler cannot find, such as Net in a project without the
/// Networking module, beside the compiler's own error that the name does not exist.
/// </summary>
[DiagnosticAnalyzer(LanguageNames.CSharp)]
public sealed class ModuleAnalyzer : DiagnosticAnalyzer
{
	public override ImmutableArray<DiagnosticDescriptor> SupportedDiagnostics => ImmutableArray.Create(Diagnostics.ModuleOff);

	public override void Initialize(AnalysisContext context)
	{
		context.ConfigureGeneratedCodeAnalysis(GeneratedCodeAnalysisFlags.None);
		context.EnableConcurrentExecution();
		context.RegisterSyntaxNodeAction(Analyze, SyntaxKind.IdentifierName);
	}

	private static void Analyze(SyntaxNodeAnalysisContext context)
	{
		var name = ((IdentifierNameSyntax)context.Node).Identifier.ValueText;
		var module = ModuleNames.Of(name);
		if (module == null)
		{
			return;
		}

		var found = context.SemanticModel.GetSymbolInfo(context.Node, context.CancellationToken);
		if (!Exists(found.Symbol) && !found.CandidateSymbols.Any(Exists))
		{
			context.ReportDiagnostic(Diagnostic.Create(Diagnostics.ModuleOff, context.Node.GetLocation(), name, module));
		}
	}

	/// <summary>Whether the compiler found a symbol for real, rather than standing in for a missing type.</summary>
	private static bool Exists(ISymbol? symbol)
	{
		return symbol != null && symbol.Kind != SymbolKind.ErrorType;
	}
}
