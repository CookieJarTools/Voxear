using System;
using System.IO;
using System.Reflection;
using System.Runtime.Loader;
using Voxear.Scripting;

namespace Voxear.ScriptInfo
{
	/// <summary>
	/// Writes what a built game assembly's behaviours are, as JSON, for the editor to show their fields:
	/// dotnet Voxear.ScriptInfo.dll Game.dll Behaviours.json. It runs as a process of its own, so making behaviours to
	/// read the values they start with cannot touch the editor, and a crash here costs only the description.
	/// </summary>
	internal static class Program
	{
		private static int Main(string[] args)
		{
			if (args.Length != 2)
			{
				Console.Error.WriteLine("usage: Voxear.ScriptInfo <game assembly> <output json>");
				return 2;
			}

			var assembly = Path.GetFullPath(args[0]);
			var folder = Path.GetDirectoryName(assembly)!;
			AssemblyLoadContext.Default.Resolving += (context, name) => Probe(context, folder, name);
			var game = AssemblyLoadContext.Default.LoadFromAssemblyPath(assembly);

			// Written beside the output and moved over it, so a reader never sees half a file.
			var partial = args[1] + ".partial";
			File.WriteAllText(partial, ScriptCatalog.Describe(game));
			File.Move(partial, args[1], overwrite: true);
			return 0;
		}

		/// <summary>The game's own dependencies sit beside it; Voxear.Core is this tool's.</summary>
		private static Assembly? Probe(AssemblyLoadContext context, string folder, AssemblyName name)
		{
			var path = Path.Combine(folder, name.Name + ".dll");
			return File.Exists(path) ? context.LoadFromAssemblyPath(path) : null;
		}
	}
}
