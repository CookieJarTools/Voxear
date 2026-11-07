using System;
using System.Collections.Immutable;
using System.IO;
using System.Linq;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.Diagnostics;

namespace Voxear.Analyzers.Tests;

internal static class ModuleTests
{
	private const string UsesNetworking = """
		using Voxear;
		[Predicted]
		public sealed partial class Runner : Behaviour
		{
			[Replicated] public partial int Laps { get; set; }
			protected override void Update()
			{
				foreach (var player in Net.Players) { }
				NetObject net = NetObject.None;
			}
		}
		""";

	private static void TestANetworkingNameWithoutTheModuleSaysWhichModule()
	{
		var said = Analyze(UsesNetworking, withNetworking: false);
		var names = said.Select(d => d.GetMessage().Split(' ')[0]).Distinct().OrderBy(n => n).ToList();
		var wanted = new[] { "Net", "NetObject", "Predicted", "Replicated" };
		if (!names.SequenceEqual(wanted) || said.Any(d => !d.GetMessage().Contains("the Networking module")))
		{
			throw new Exception($"wanted {string.Join(", ", wanted)}, got {string.Join("; ", said.Select(d => d.GetMessage()))}");
		}
	}

	private static void TestTheSameNamesWithTheModuleSayNothing()
	{
		var said = Analyze(UsesNetworking, withNetworking: true);
		if (said.Length > 0)
		{
			throw new Exception($"wanted nothing, got {string.Join("; ", said.Select(d => d.GetMessage()))}");
		}
	}

	/// <summary>What the module analyzer says of code built against Voxear.Core, with or without Voxear.Networking.</summary>
	private static ImmutableArray<Diagnostic> Analyze(string code, bool withNetworking)
	{
		// The test's own references include the engine's assemblies; the game gets only those it is given.
		var references = ((string)AppContext.GetData("TRUSTED_PLATFORM_ASSEMBLIES")!)
			.Split(Path.PathSeparator)
			.Where(path => !Path.GetFileName(path).StartsWith("Voxear.", StringComparison.Ordinal))
			.Select(path => MetadataReference.CreateFromFile(path))
			.Append(MetadataReference.CreateFromFile(typeof(Voxear.Behaviour).Assembly.Location));
		if (withNetworking)
		{
			references = references.Append(MetadataReference.CreateFromFile(typeof(Voxear.Net).Assembly.Location));
		}

		var compilation = CSharpCompilation.Create(
			"Game",
			[CSharpSyntaxTree.ParseText(code)],
			references,
			new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary));
		var analyzers = ImmutableArray.Create<DiagnosticAnalyzer>(new ModuleAnalyzer());
		return compilation.WithAnalyzers(analyzers).GetAnalyzerDiagnosticsAsync().Result;
	}
}
