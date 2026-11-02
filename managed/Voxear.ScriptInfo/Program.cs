using System;
using System.IO;
using System.Reflection;
using System.Runtime.Loader;
using Voxear.Scripting;

namespace Voxear.ScriptInfo;

/// <summary>
/// Writes what a built game assembly's behaviours are, as JSON, for the editor to show their fields:
/// dotnet Voxear.ScriptInfo.dll Game.dll Behaviours.json [Code.json]. With a third path it also writes the Code
/// Reference, the public types of the game and of Voxear.Core with their comments, for VoxDocs. It runs as a
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
		var game = AssemblyLoadContext.Default.LoadFromAssemblyPath(assembly);

		Write(args[1], ScriptCatalog.Describe(game));
		if (args.Length == 3)
		{
			Write(args[2], CodeReference.Describe(new[] { game, typeof(SerializeFieldAttribute).Assembly }));
		}

		return 0;
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
