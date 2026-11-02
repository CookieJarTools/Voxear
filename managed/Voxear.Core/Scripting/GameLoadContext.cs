using System.Reflection;
using System.Runtime.Loader;

namespace Voxear.Scripting;

/// <summary>
/// Game scripts load here rather than into the default context so that the whole set can be
/// unloaded and loaded again when scripts change. Voxear.Core always comes from the default
/// context, so Behaviour means the same type on both sides of that boundary.
/// </summary>
internal sealed class GameLoadContext : AssemblyLoadContext
{
	private readonly AssemblyDependencyResolver resolver;

	public GameLoadContext(string mainAssembly) : base("Voxear.Game", isCollectible: true)
	{
		resolver = new AssemblyDependencyResolver(mainAssembly);
	}

	protected override Assembly? Load(AssemblyName name)
	{
		if (name.Name == typeof(Behaviour).Assembly.GetName().Name)
		{
			return null;
		}

		var path = resolver.ResolveAssemblyToPath(name);
		return path == null ? null : LoadFromAssemblyPath(path);
	}
}
