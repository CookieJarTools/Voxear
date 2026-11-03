using System.Collections.Generic;
using System.IO;

namespace Voxear.Scripting;

/// <summary>
/// The assemblies of the project's packages, which load with the game's scripts so that their behaviours, editor
/// windows, drawers and commands are found as the game's own are. A build of the scripts lists them in a file beside
/// the game's assembly, one file name a line, each after the ones it uses; a project without packages has no list.
/// </summary>
internal static class PackageAssemblies
{
	/// <summary>The list's file name, as the engine writes it beside the game's assembly.</summary>
	public const string ListFile = "Packages.txt";

	/// <summary>The full paths of the package assemblies listed beside the game's assembly at this path.</summary>
	public static IReadOnlyList<string> Beside(string gameAssembly)
	{
		var folder = Path.GetDirectoryName(Path.GetFullPath(gameAssembly))!;
		var list = Path.Combine(folder, ListFile);
		if (!File.Exists(list))
		{
			return [];
		}

		var paths = new List<string>();
		foreach (var line in File.ReadAllLines(list))
		{
			var name = line.Trim();
			if (name.Length > 0)
			{
				paths.Add(Path.Combine(folder, name));
			}
		}
		return paths;
	}
}
