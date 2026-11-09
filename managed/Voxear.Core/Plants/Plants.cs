using System;
using System.Collections.Generic;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>Every Foliage's plants at once: finding those that stand near a point.</summary>
public static unsafe class Plants
{
	/// <summary>
	/// Fills <paramref name="found"/> with the plants standing within <paramref name="radius"/> metres of
	/// <paramref name="centre"/>, nearest first, and returns how many there are.
	/// </summary>
	public static int Query(Vector3 centre, float radius, List<Plant> found)
	{
		found.Clear();
		var count = Native.Api->PlantsNear(&centre, radius, null, 0);
		if (count <= 0)
		{
			return 0;
		}
		var buffer = new PlantRefData[count];
		fixed (PlantRefData* into = buffer)
		{
			count = Math.Min(count, Native.Api->PlantsNear(&centre, radius, into, buffer.Length));
		}
		for (var index = 0; index < count; index++)
		{
			found.Add(new Plant(new Obj(buffer[index].Obj), (int)buffer[index].Number));
		}
		return count;
	}
}
