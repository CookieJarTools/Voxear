using System;
using System.Collections.Generic;
using System.Numerics;
using System.Reflection;
using Voxear.Scripting;

namespace Voxear;

/// <summary>Moves every live behaviour's [WorldPosition] fields as the world's middle moves.</summary>
internal static class WorldPositions
{
	private const BindingFlags Declared = BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.DeclaredOnly;

	private static readonly Dictionary<Type, FieldInfo[]> marked = [];

	public static void Move(Vector3 by)
	{
		foreach (var behaviour in BehaviourTable.Live())
		{
			foreach (var field in Marked(behaviour.GetType()))
			{
				MoveField(behaviour, field, by);
			}
		}
	}

	public static void Forget()
	{
		marked.Clear();
	}

	private static void MoveField(Behaviour behaviour, FieldInfo field, Vector3 by)
	{
		switch (field.GetValue(behaviour))
		{
			case Vector3 at:
				field.SetValue(behaviour, at + by);
				break;
			case Vector3[] all:
				for (var index = 0; index < all.Length; index++)
				{
					all[index] += by;
				}
				break;
			case List<Vector3> all:
				for (var index = 0; index < all.Count; index++)
				{
					all[index] += by;
				}
				break;
		}
	}

	/// <summary>A behaviour type's fields marked [WorldPosition], its base classes' too.</summary>
	private static FieldInfo[] Marked(Type type)
	{
		if (marked.TryGetValue(type, out var known))
		{
			return known;
		}

		var found = new List<FieldInfo>();
		for (var each = type; each != null && each != typeof(Behaviour); each = each.BaseType)
		{
			found.AddRange(Array.FindAll(each.GetFields(Declared), field => field.IsDefined(typeof(WorldPositionAttribute))));
		}
		marked[type] = [.. found];
		return marked[type];
	}
}
