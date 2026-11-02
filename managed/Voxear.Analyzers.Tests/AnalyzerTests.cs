using System;
using System.Collections.Immutable;
using System.IO;
using System.Linq;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.Diagnostics;

namespace Voxear.Analyzers.Tests;

internal static class AnalyzerTests
{
	private static void TestRightAttributesSayNothing()
	{
		const string code = """
			using System.Collections.Generic;
			using Voxear;
			public sealed class Tower : Behaviour
			{
				public bool CanFly;
				[ShowIf(nameof(CanFly))] [Range(0, 20)] public float Lift;
				[OnValueChanged(nameof(Rebuild))] public int Floors;
				[ValidateInput(nameof(IsColour))] [ValueDropdown(nameof(Colours))] public string Paint = "Red";
				[ColorPalette("#e63946", "457b9d")] public Color Tint;
				[InfoBox("Flies", InfoMessageType.Info, nameof(CanFly))] public int Note;
				[Button] private void Rebuild() { }
				private bool IsColour(string paint) { return paint.Length > 0; }
				private IEnumerable<string> Colours() { yield return "Red"; }
			}
			""";
		Expect(code);
	}

	private static void TestWrongNamesAndShapesAreErrors()
	{
		const string code = """
			using Voxear;
			public sealed class Tower : Behaviour
			{
				[ShowIf("canFly")] public float Lift;
				[OnValueChanged("Rebuild")] public int Floors;
				[ValidateInput("IsColour")] public string Paint = "Red";
				[ValueDropdown("Count")] public string Size;
				[Button] private void Rebuild(int floors) { }
				private bool IsColour(int paint) { return true; }
				private int Count;
			}
			""";
		Expect(code, "VOX0001", "VOX0002", "VOX0003", "VOX0004", "VOX0005");
	}

	private static void TestNumbersOnTextAndBadColoursAreFound()
	{
		const string code = """
			using Voxear;
			public sealed class Tower : Behaviour
			{
				[Range(0, 1)] public string Name;
				[ColorPalette("#12345", "red")] public Color Tint;
			}
			""";
		Expect(code, "VOX0006", "VOX0007", "VOX0007");
	}

	/// <summary>Compiles code against Voxear.Core and checks the analyzer reports exactly these, in any order.</summary>
	private static void Expect(string code, params string[] ids)
	{
		var references = ((string)AppContext.GetData("TRUSTED_PLATFORM_ASSEMBLIES")!)
			.Split(Path.PathSeparator)
			.Select(path => MetadataReference.CreateFromFile(path))
			.Append(MetadataReference.CreateFromFile(typeof(Voxear.Behaviour).Assembly.Location));
		var compilation = CSharpCompilation.Create(
			"Game",
			new[] { CSharpSyntaxTree.ParseText(code) },
			references,
			new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary, nullableContextOptions: NullableContextOptions.Enable));
		var errors = compilation.GetDiagnostics().Where(d => d.Severity == DiagnosticSeverity.Error).ToList();
		if (errors.Count > 0)
		{
			throw new Exception($"the snippet does not compile: {string.Join("; ", errors)}");
		}

		var analyzers = ImmutableArray.Create<DiagnosticAnalyzer>(new InspectorAttributeAnalyzer());
		var found = compilation.WithAnalyzers(analyzers).GetAnalyzerDiagnosticsAsync().Result;
		var got = found.Select(d => d.Id).OrderBy(id => id).ToList();
		var wanted = ids.OrderBy(id => id).ToList();
		if (!got.SequenceEqual(wanted))
		{
			throw new Exception($"wanted {string.Join(", ", wanted)}, got {string.Join("; ", found.Select(d => $"{d.Id} {d.GetMessage()}"))}");
		}
	}
}
