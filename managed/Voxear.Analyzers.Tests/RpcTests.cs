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
				[ClientRpc]
				private void Joined(byte[] ground) { }
				private void Use(NetPlayer player)
				{
					CastOnServer(Vector3.UnitX, Weapon.Staff, Obj);
					WaveOnServer();
					BlastedOnClients(player, "hi");
					BlastedOnClient(player, player, "only you");
					JoinedOnClient(player, new byte[] { 1, 2 });
				}
			}
			""";
		var (generated, diagnostics) = Run(code);
		if (diagnostics.Length > 0)
		{
			throw new Exception($"wanted nothing, got {string.Join("; ", diagnostics.Select(d => d.ToString()))}");
		}
		var checks = new[] { "MayRunOnServer(this, key, false, 5)", "MayRunOnServer(this, key, true, 0)", "MayRunOnPlayer()", "reader.ReadBytes()" };
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

	private static void TestWorldPositionsGoFromTheLevelMiddle()
	{
		const string code = """
			using System.Numerics;
			using Voxear;
			namespace Game;
			public sealed partial class Flag : Behaviour
			{
				[Replicated, WorldPosition] public partial Vector3 Rally { get; set; }
				[Replicated] public partial Vector3 Wind { get; set; }
				[ServerRpc]
				private void Signal([WorldPosition] Vector3 at, Vector3 towards) { }
				private void Use()
				{
					SignalOnServer(Rally, Wind);
				}
			}
			""";
		// The compiler these tests use predates partial properties, so only what the generators say counts.
		var (generated, diagnostics) = Run(code, new ReplicatedGenerator());
		var said = diagnostics.Where(d => d.Id.StartsWith("VOX", StringComparison.Ordinal)).ToList();
		if (said.Count > 0)
		{
			throw new Exception($"wanted nothing, got {string.Join("; ", said.Select(d => d.ToString()))}");
		}
		var checks = new[]
		{
			"writer.Write(global::Voxear.Origin.ToLevel(replicatedRally))",
			"writer.Write(replicatedWind)",
			"[global::Voxear.WorldPosition]\n\tprivate global::System.Numerics.Vector3 replicatedRally;",
			"writer.Write(global::Voxear.Origin.ToLevel(@at))",
			"writer.Write(@towards)",
			"global::Voxear.Origin.FromLevel(reader.ReadLevelPosition()), reader.ReadVector3()",
		};
		var missing = checks.Where(check => !generated.Replace("\r\n", "\n").Contains(check)).ToList();
		if (missing.Count > 0)
		{
			throw new Exception($"missing {string.Join(" | ", missing)} in:\n{generated}");
		}
	}

	/// <summary>Runs the generator over code compiled against Voxear.Core: what it wrote, and the errors after.</summary>
	private static (string Generated, Diagnostic[] Diagnostics) Run(string code, IIncrementalGenerator? also = null)
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
		IIncrementalGenerator[] generators = also == null ? [new RpcGenerator()] : [new RpcGenerator(), also];
		var driver = CSharpGeneratorDriver.Create(generators).RunGeneratorsAndUpdateCompilation(compilation, out var output, out var reported);
		var generated = string.Join("\n", driver.GetRunResult().GeneratedTrees.Select(tree => tree.ToString()));
		var errors = output.GetDiagnostics().Where(d => d.Severity == DiagnosticSeverity.Error);
		return (generated, reported.Concat(errors).ToArray());
	}
}
