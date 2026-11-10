using System;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using System.Text;

namespace Voxear;

/// <summary>Finds every [ContentRoot] in the game scripts, for the content build.</summary>
internal static class ContentRoots
{
	private const BindingFlags Members =
		BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Static | BindingFlags.Instance |
		BindingFlags.DeclaredOnly;

	/// <summary>What comes before a Definition's name in the pattern of a root that is every data asset holding it.</summary>
	public const string TypeRoot = "type:";

	/// <summary>
	/// One line per pattern: the pattern, a tab, then the type or member that declares it. A Definition marked
	/// [ContentRoot] with no pattern, or deriving from one that is, gives a line for its every data asset: the
	/// engine's own among them, as a recipe is.
	/// </summary>
	public static string Describe(IEnumerable<Assembly> assemblies)
	{
		var lines = new StringBuilder();
		var scripts = assemblies.ToList();
		foreach (var (pattern, source) in scripts.SelectMany(Find).Concat(WholeDefinitions(scripts)))
		{
			lines.Append(pattern).Append('\t').Append(source).Append('\n');
		}

		return lines.ToString();
	}

	/// <summary>
	/// The Definitions whose every data asset ships: those marked [ContentRoot] with no pattern, and those deriving
	/// from them, in <paramref name="assemblies"/> and the engine's own.
	/// </summary>
	public static List<(string Pattern, string Source)> WholeDefinitions(IReadOnlyList<Assembly> assemblies)
	{
		var found = new List<(string Pattern, string Source)>();
		var engine = typeof(Definition).Assembly;
		foreach (var type in assemblies.Append(engine).Distinct().SelectMany(LoadableTypes))
		{
			var whole = type.GetCustomAttributes(typeof(ContentRootAttribute), true).Cast<ContentRootAttribute>();
			if (typeof(Definition).IsAssignableFrom(type) && !type.IsAbstract && whole.Any(root => root.Patterns.Count == 0))
			{
				found.Add((TypeRoot + type.FullName, type.FullName ?? type.Name));
			}
		}
		return found;
	}

	public static List<(string Pattern, string Source)> Find(Assembly assembly)
	{
		var found = new List<(string Pattern, string Source)>();
		Add(found, assembly, $"assembly {assembly.GetName().Name}");
		foreach (var type in LoadableTypes(assembly))
		{
			var name = type.FullName ?? type.Name;
			Add(found, type, name);
			foreach (var member in type.GetMembers(Members).Where(member => member is not Type))
			{
				Add(found, member, $"{name}.{member.Name}");
			}
		}

		return found;
	}

	private static void Add(List<(string Pattern, string Source)> found, ICustomAttributeProvider provider, string source)
	{
		var attributes = provider.GetCustomAttributes(typeof(ContentRootAttribute), false).Cast<ContentRootAttribute>();
		found.AddRange(attributes.SelectMany(attribute => attribute.Patterns).Select(pattern => (pattern, source)));
	}

	/// <summary>
	/// The types that loaded. A type whose dependencies are missing cannot declare anything the game can run,
	/// so it is skipped rather than failing the build.
	/// </summary>
	private static IEnumerable<Type> LoadableTypes(Assembly assembly)
	{
		try
		{
			return assembly.GetTypes();
		}
		catch (ReflectionTypeLoadException exception)
		{
			return exception.Types.OfType<Type>();
		}
	}
}
