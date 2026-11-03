using System.IO;
using System.Linq;
using Voxear.Scripting;

namespace Voxear.Tests;

internal static class PackageAssemblyTests
{
	[Test]
	private static void PackageAssembliesAreReadFromTheListBesideTheGames()
	{
		var folder = Directory.CreateTempSubdirectory("voxear-packages-");
		try
		{
			var game = Path.Combine(folder.FullName, "Orbits.Scripts.dll");
			Check.True(PackageAssemblies.Beside(game).Count == 0, "a project without packages has no list");

			File.WriteAllText(Path.Combine(folder.FullName, PackageAssemblies.ListFile), "OrbitCore.dll\n\nOrbitTools.dll\n");
			var names = PackageAssemblies.Beside(game).Select(Path.GetFileName).ToList();
			Check.True(names.SequenceEqual(["OrbitCore.dll", "OrbitTools.dll"]), string.Join(", ", names));
			Check.True(PackageAssemblies.Beside(game).All(path => Path.GetDirectoryName(path) == folder.FullName), "beside the game's assembly");
		}
		finally
		{
			folder.Delete(recursive: true);
		}
	}
}
