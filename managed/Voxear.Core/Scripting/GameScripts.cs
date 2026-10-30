using System.Collections.Generic;
using System.Reflection;

namespace Voxear.Scripting
{
	/// <summary>The game assemblies, all loaded into one collectible context.</summary>
	internal static class GameScripts
	{
		private static readonly List<Assembly> assemblies = new List<Assembly>();
		private static GameLoadContext? context;

		public static IReadOnlyList<Assembly> Assemblies => assemblies;

		public static void Load(string path)
		{
			context ??= new GameLoadContext(path);
			assemblies.Add(context.LoadFromAssemblyPath(path));
			BehaviourTypes.Clear();
			Log.Info($"Loaded game scripts from {path}");
		}
	}
}
