using System;
using System.Collections.Generic;
using System.Collections.Immutable;
using System.IO;
using System.Linq;
using System.Reflection;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;

namespace Voxear.Analyzers.Tests;

internal static class StringKeysTests
{
	private const string Menu = """"
		format = 1
		sheet = "https://example.com/menu.csv"

		[[entry]]
		key = "menu.play"
		comment = "The main menu's first button"
		limit = 12
		en = "Play"
		fr = "Jouer"
		pt-BR = "Jogar"

		[[entry]]
		key = "menu.high-score"
		en = 'Best <score> & "more"'

		[[entry]]
		key = "menu.to-string"
		en = "Say \"hi\"\\ é"
		"zh Hant" = "x"

		[[entry]]
		key = "hud.1st_place"
		notes = """
		[[entry]]
		key = "not.a.key"
		"""
		en = "First"
		"""";

	private const string Hud = """
		format = 1

		[[entry]]
		key = "hud.1st_place"
		en = "First, as the first table has it"

		[[entry]]
		key = 'hud.lap'
		en = "Lap {lap}"
		""";

	private const string Clashes = """
		[[entry]]
		key = "menu"
		en = "Menu"

		[[entry]]
		key = "menu.play"

		[[entry]]
		key = "score.high-score"

		[[entry]]
		key = "score.high_score"

		[[entry]]
		key = "strings.title"

		[[entry]]
		key = "list.list"

		[[entry]]
		key = "!!!"
		""";

	private static readonly CSharpParseOptions Parse = new(LanguageVersion.Latest, DocumentationMode.Diagnose);

	private static void TestTablesMakeNestedKeysOnce()
	{
		const string code = """
			namespace Game;
			internal static class Menus
			{
				public static Voxear.LocalizedString Play() { return Strings.Menu.Play; }
			}
			""";
		var (compilation, warnings, source) = Generate(code, "Game", new TableText("Text/Menu.strings", Menu), new TableText("Text/Hud.strings", Hud));
		ExpectClean(compilation, warnings);
		var assembly = Load(compilation);
		ExpectKey(assembly, "Game.Strings+Menu", "Play", "menu.play");
		ExpectKey(assembly, "Game.Strings+Menu", "HighScore", "menu.high-score");
		ExpectKey(assembly, "Game.Strings+Menu", "ToString", "menu.to-string");
		ExpectKey(assembly, "Game.Strings+Hud", "_1stPlace", "hud.1st_place");
		ExpectKey(assembly, "Game.Strings+Hud", "Lap", "hud.lap");
		ExpectIn(source, "namespace Game;", "/// Play\n", "/// <para>The main menu's first button</para>", "/// Best &lt;score&gt; &amp; \"more\"", "/// Say \"hi\"\\ é\n", "/// First, as the first table has it");
		if (source.Contains("NotAKey") || source.Contains("not.a.key"))
		{
			throw new Exception($"a string over several lines was read as entries:\n{source}");
		}
	}

	private static void TestAMissingKeyDoesNotCompile()
	{
		const string code = """
			namespace Game;
			internal static class Menus
			{
				public static Voxear.LocalizedString Quit() { return Strings.Menu.Quit; }
			}
			""";
		var (compilation, _, _) = Generate(code, "Game", new TableText("Text/Menu.strings", Menu));
		var errors = compilation.GetDiagnostics().Where(d => d.Severity == DiagnosticSeverity.Error).Select(d => d.Id).ToList();
		if (!errors.SequenceEqual(["CS0117"]))
		{
			throw new Exception($"wanted CS0117 for Strings.Menu.Quit, got {string.Join(", ", errors)}");
		}
	}

	private static void TestClashingKeysAreWarnedAndLeftOut()
	{
		const string code = """
			internal static class Menus
			{
				public static Voxear.LocalizedString Play() { return Strings.Menu.Play; }
			}
			""";
		var (compilation, warnings, _) = Generate(code, string.Empty, new TableText("Text/Clashes.strings", Clashes));
		var errors = compilation.GetDiagnostics().Where(d => d.Severity >= DiagnosticSeverity.Warning).ToList();
		if (errors.Count > 0)
		{
			throw new Exception($"what is left does not compile: {string.Join("; ", errors)}");
		}
		var left = warnings.Select(d => (d.Id, Key: d.GetMessage().Split('"')[1])).OrderBy(d => d.Key, StringComparer.Ordinal).ToList();
		List<(string, string)> wanted = [("VOX0008", "!!!"), ("VOX0008", "list.list"), ("VOX0008", "menu"), ("VOX0008", "score.high-score"), ("VOX0008", "score.high_score"), ("VOX0008", "strings.title")];
		if (!left.SequenceEqual(wanted))
		{
			throw new Exception($"wanted {string.Join(", ", wanted)}, got {string.Join("; ", warnings.Select(d => d.GetMessage()))}");
		}
		var menu = warnings.Single(d => d.GetMessage().StartsWith("\"menu\"", StringComparison.Ordinal)).Location.GetLineSpan();
		if (menu.Path != "Text/Clashes.strings" || menu.StartLinePosition.Line != 1)
		{
			throw new Exception($"the warning for menu is at {menu}, not its key's line");
		}
	}

	/// <summary>Compiles code with what the generator makes of the tables, in a project whose RootNamespace is rootNamespace.</summary>
	private static (Compilation Compilation, ImmutableArray<Diagnostic> Warnings, string Source) Generate(string code, string rootNamespace, params TableText[] tables)
	{
		var references = ((string)AppContext.GetData("TRUSTED_PLATFORM_ASSEMBLIES")!)
			.Split(Path.PathSeparator)
			.Select(path => MetadataReference.CreateFromFile(path))
			.Append(MetadataReference.CreateFromFile(typeof(Voxear.LocalizedString).Assembly.Location));
		var compilation = CSharpCompilation.Create(
			"Game",
			[CSharpSyntaxTree.ParseText(code, Parse)],
			references,
			new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary, nullableContextOptions: NullableContextOptions.Enable));
		var properties = new BuildProperties(new() { ["build_property.RootNamespace"] = rootNamespace });
		var driver = CSharpGeneratorDriver.Create([new StringKeysGenerator().AsSourceGenerator()], tables, Parse, new BuildPropertiesProvider(properties));
		driver.RunGeneratorsAndUpdateCompilation(compilation, out var output, out var warnings);
		var source = output.SyntaxTrees.Skip(1).Select(tree => tree.ToString()).SingleOrDefault() ?? string.Empty;
		return (output, warnings, source);
	}

	private static void ExpectClean(Compilation compilation, ImmutableArray<Diagnostic> warnings)
	{
		var found = warnings.Concat(compilation.GetDiagnostics().Where(d => d.Severity >= DiagnosticSeverity.Warning)).ToList();
		if (found.Count > 0)
		{
			throw new Exception($"wanted nothing said, got {string.Join("; ", found)}");
		}
	}

	private static Assembly Load(Compilation compilation)
	{
		using var built = new MemoryStream();
		var emitted = compilation.Emit(built);
		if (!emitted.Success)
		{
			throw new Exception($"it does not build: {string.Join("; ", emitted.Diagnostics)}");
		}
		return Assembly.Load(built.ToArray());
	}

	/// <summary>Checks a generated member is the LocalizedString for key.</summary>
	private static void ExpectKey(Assembly assembly, string type, string member, string key)
	{
		var property = assembly.GetType(type)?.GetProperty(member, BindingFlags.Public | BindingFlags.Static)
			?? throw new Exception($"{type} has no {member}");
		var value = (Voxear.LocalizedString)property.GetValue(null)!;
		if (value.Key != key)
		{
			throw new Exception($"{type}.{member} is \"{value.Key}\", not \"{key}\"");
		}
	}

	private static void ExpectIn(string source, params string[] parts)
	{
		var missing = parts.Where(part => !source.Contains(part)).ToList();
		if (missing.Count > 0)
		{
			throw new Exception($"the generated Strings has no {string.Join(", ", missing)}:\n{source}");
		}
	}
}
