using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Runtime.Loader;
using Voxear.Scripting;

namespace Voxear.ScriptInfo;

/// <summary>
/// Writes what a built game assembly's behaviours are, as JSON, for the editor to show their fields:
/// dotnet Voxear.ScriptInfo.dll Game.dll Behaviours.json [Code.json]. The assemblies of the project's packages,
/// listed beside the game's, are described with it. With a third path it also writes the Code Reference, the public
/// types of the game, its packages and Voxear.Core with their comments, for VoxDocs. It runs as a
/// process of its own, so making behaviours to read the values they start with cannot touch the editor, and a
/// crash here costs only the description.
/// </summary>
internal static class Program
{
	private static int Main(string[] args)
	{
		if (args.Length != 2 && args.Length != 3)
		{
			Console.Error.WriteLine("usage: Voxear.ScriptInfo <game assembly> <output json> [<code reference json>]");
			return 2;
		}

		var assembly = Path.GetFullPath(args[0]);
		var folder = Path.GetDirectoryName(assembly)!;
		AssemblyLoadContext.Default.Resolving += (context, name) => Probe(context, folder, name);
		var scripts = new List<Assembly>();
		foreach (var package in PackageAssemblies.Beside(assembly))
		{
			scripts.Add(AssemblyLoadContext.Default.LoadFromAssemblyPath(package));
		}
		scripts.Add(AssemblyLoadContext.Default.LoadFromAssemblyPath(assembly));

		Write(args[1], ScriptCatalog.Describe(scripts));
		if (args.Length == 3)
		{
			Write(args[2], CodeReference.Describe([.. scripts, typeof(SerializeFieldAttribute).Assembly, .. Modules(scripts)]));
		}

		return 0;
	}

	/// <summary>The engine's modules the scripts use, such as Voxear.Networking, whose types the reference shows too.</summary>
	private static IEnumerable<Assembly> Modules(IEnumerable<Assembly> scripts)
	{
		var core = typeof(SerializeFieldAttribute).Assembly.GetName().Name;
		return scripts
			.SelectMany(assembly => assembly.GetReferencedAssemblies())
			.Where(name => name.Name != null && name.Name.StartsWith("Voxear.", StringComparison.Ordinal) && name.Name != core)
			.Select(name => name.Name!)
			.Distinct()
			.Select(name => Assembly.Load(new AssemblyName(name)));
	}

	/// <summary>Written beside its path and moved over it, so a reader never sees half a file.</summary>
	private static void Write(string path, string text)
	{
		var partial = path + ".partial";
		File.WriteAllText(partial, text);
		File.Move(partial, path, overwrite: true);
	}

	/// <summary>The game's own dependencies sit beside it; Voxear.Core is this tool's.</summary>
	private static Assembly? Probe(AssemblyLoadContext context, string folder, AssemblyName name)
	{
		var path = Path.Combine(folder, name.Name + ".dll");
		return File.Exists(path) ? context.LoadFromAssemblyPath(path) : null;
	}
}
