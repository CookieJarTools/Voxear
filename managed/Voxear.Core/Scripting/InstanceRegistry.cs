using System;
using System.Collections.Generic;

namespace Voxear.Scripting
{
	/// <summary>The live IInstance behaviours, one per type.</summary>
	internal static class InstanceRegistry
	{
		private static readonly Dictionary<Type, Behaviour> instances = new Dictionary<Type, Behaviour>();

		/// <summary>Changes whenever an instance comes or goes, so Instance of T can cache its lookup.</summary>
		public static int Version { get; private set; }

		public static void Register(Behaviour behaviour)
		{
			var type = behaviour.GetType();
			if (instances.TryGetValue(type, out var existing))
			{
				Log.Warning($"{type.Name} is an IInstance and {existing.Obj.Name} already holds it, so the one on {behaviour.Obj.Name} is ignored.");
				return;
			}

			instances[type] = behaviour;
			Version++;
		}

		public static void Unregister(Behaviour behaviour)
		{
			var type = behaviour.GetType();
			if (instances.TryGetValue(type, out var existing) && existing == behaviour)
			{
				instances.Remove(type);
				Version++;
			}
		}

		/// <summary>Forgets every instance, as a reload of the scripts does before registering their replacements.</summary>
		public static void Clear()
		{
			instances.Clear();
			Version++;
		}

		public static Behaviour? Get(Type type)
		{
			return instances.TryGetValue(type, out var behaviour) ? behaviour : null;
		}
	}
}
