using System.Collections.Generic;
using System.IO;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.Loader;

namespace Voxear.Scripting;

/// <summary>
/// The engine's optional modules' assemblies, such as Voxear.Networking. Each sits beside Voxear.Core and is shared, as
/// it is, from the default context, so it is never unloaded with the game's scripts. A game's build references only the
/// modules its project has, so a module's assembly is read the first time a game's assembly names it, and its module
/// initializer adds it to <see cref="ModuleHooks"/>.
/// </summary>
internal static class ModuleAssemblies
{
	private const string Prefix = "Voxear.";

	private static readonly HashSet<string> loaded = [];

	/// <summary>Whether an assembly name is one of the engine's own, which comes from the default context.</summary>
	public static bool IsEngine(AssemblyName name)
	{
		return name.Name != null && name.Name.StartsWith(Prefix) && File.Exists(PathOf(name.Name));
	}

	/// <summary>Reads the modules an assembly references that are not read yet.</summary>
	public static void LoadFor(Assembly assembly)
	{
		foreach (var name in assembly.GetReferencedAssemblies())
		{
			if (name.Name == null || loaded.Contains(name.Name) || !IsEngine(name) || name.Name == Core.GetName().Name)
			{
				continue;
			}

			var module = AssemblyLoadContext.Default.LoadFromAssemblyPath(PathOf(name.Name));
			loaded.Add(name.Name);
			RuntimeHelpers.RunModuleConstructor(module.ManifestModule.ModuleHandle);
		}
	}

	private static Assembly Core => typeof(Behaviour).Assembly;

	private static string PathOf(string name)
	{
		return Path.Combine(Path.GetDirectoryName(Core.Location) ?? "", name + ".dll");
	}
}
