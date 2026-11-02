using System;
using System.Collections.Generic;
using System.Diagnostics.CodeAnalysis;

namespace Voxear.Scripting;

/// <summary>Finds behaviour classes by full name in the game scripts, then in Voxear.Core.</summary>
internal static class BehaviourTypes
{
	private static readonly Dictionary<string, BehaviourType?> cache = new Dictionary<string, BehaviourType?>();

	public static BehaviourType? Find(string name)
	{
		if (!cache.TryGetValue(name, out var found))
		{
			found = Resolve(name);
			cache[name] = found;
		}
		return found;
	}

	public static void Clear()
	{
		cache.Clear();
	}

	private static BehaviourType? Resolve(string name)
	{
		foreach (var assembly in GameScripts.Assemblies)
		{
			var type = assembly.GetType(name);
			if (IsBehaviour(type))
			{
				return new BehaviourType(type);
			}
		}

		var core = typeof(Behaviour).Assembly.GetType(name);
		return IsBehaviour(core) ? new BehaviourType(core) : null;
	}

	private static bool IsBehaviour([NotNullWhen(true)] Type? type)
	{
		return type != null && !type.IsAbstract && typeof(Behaviour).IsAssignableFrom(type);
	}
}
