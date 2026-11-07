using System;
using System.IO;
using System.Linq;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;

namespace Voxear.Analyzers.Tests;

internal static class RpcTests
{
	private static void TestSendersAreWrittenAndCompile()
	{
		const string code = """
			using System.Numerics;
			using Voxear;
			namespace Game;
			public enum Weapon { Wand, Staff }
			public sealed partial class Caster : Behaviour
			{
				[ServerRpc(PerSecond = 5)]
				private void Cast(Vector3 aim, Weapon weapon, Obj target, int power = 2) { }
				[ServerRpc(AnyPlayer = true, Unreliable = true)]
				public void Wave() { }
				[ClientRpc]
				private void Blasted(NetPlayer by, string @string) { }
				private void Use(NetPlayer player)
				{
					CastOnServer(Vector3.UnitX, Weapon.Staff, Obj);
					WaveOnServer();
					BlastedOnClients(player, "hi");
					BlastedOnClient(player, player, "only you");
				}
			}
			""";
		var (generated, diagnostics) = Run(code);
		if (diagnostics.Length > 0)
		{
			throw new Exception($"wanted nothing, got {string.Join("; ", diagnostics.Select(d => d.ToString()))}");
		}
		var checks = new[] { "MayRunOnServer(this, key, false, 5)", "MayRunOnServer(this, key, true, 0)", "MayRunOnPlayer()" };
		if (!checks.All(generated.Contains))
		{
			throw new Exception($"the checks the attributes ask for are not all there:\n{generated}");
		}
	}

	private static void TestWrongRpcsAreErrors()
	{
		const string code = """
			using Voxear;
			public sealed class Plain : Behaviour
			{
				[ServerRpc] private void Fire() { }
			}
			public sealed partial class Odd : Behaviour
			{
				[ServerRpc] private int Count() { return 0; }
				[ClientRpc] private void Hold(System.Collections.Generic.List<int> list) { }
				[ServerRpc] private void Twice(int a) { }
				[ServerRpc] private void Twice(float a) { }
			}
			""";
		var (_, diagnostics) = Run(code);
		var got = diagnostics.Select(d => d.Id).Where(id => id.StartsWith("VOX", StringComparison.Ordinal)).OrderBy(id => id);
		var wanted = new[] { "VOX0011", "VOX0011", "VOX0012", "VOX0013" };
		if (!got.SequenceEqual(wanted))
		{
			throw new Exception($"wanted {string.Join(", ", wanted)}, got {string.Join("; ", diagnostics.Select(d => d.ToString()))}");
		}
	}

	/// <summary>Runs the generator over code compiled against Voxear.Core: what it wrote, and the errors after.</summary>
	private static (string Generated, Diagnostic[] Diagnostics) Run(string code)
	{
		var references = ((string)AppContext.GetData("TRUSTED_PLATFORM_ASSEMBLIES")!)
			.Split(Path.PathSeparator)
			.Select(path => MetadataReference.CreateFromFile(path))
			.Append(MetadataReference.CreateFromFile(typeof(Voxear.Behaviour).Assembly.Location))
			.Append(MetadataReference.CreateFromFile(typeof(Voxear.Net).Assembly.Location));
		var compilation = CSharpCompilation.Create(
			"Game",
			[CSharpSyntaxTree.ParseText(code, new CSharpParseOptions(LanguageVersion.Latest))],
			references,
			new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary, nullableContextOptions: NullableContextOptions.Enable));
		var driver = CSharpGeneratorDriver.Create(new RpcGenerator()).RunGeneratorsAndUpdateCompilation(compilation, out var output, out var reported);
		var generated = string.Join("\n", driver.GetRunResult().GeneratedTrees.Select(tree => tree.ToString()));
		var errors = output.GetDiagnostics().Where(d => d.Severity == DiagnosticSeverity.Error);
		return (generated, reported.Concat(errors).ToArray());
	}
}
