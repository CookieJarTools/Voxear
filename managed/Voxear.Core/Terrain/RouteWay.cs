using System;
using System.Collections.Generic;
using System.Numerics;

namespace Voxear;

/// <summary>
/// A way along routes, as Routes.Find gives it: its points in the world, how long it is in metres, and the Objs of the
/// stops it passes, in turn. A traveller follows it by distance with <see cref="At"/>.
/// <code>
/// travelled += speed * Time.Delta;
/// var (at, heading) = way.At(travelled);
/// Obj.Position = at;
/// </code>
/// </summary>
public sealed record RouteWay(IReadOnlyList<Vector3> Points, float Length, IReadOnlyList<Obj> Stops)
{
	/// <summary>The place a distance along it, and the way it runs there; its ends before and past it.</summary>
	public (Vector3 At, Vector3 Heading) At(float distance)
	{
		if (Points.Count == 0)
		{
			return (Vector3.Zero, Vector3.UnitZ);
		}
		var left = MathF.Max(0f, distance);
		for (var at = 1; at < Points.Count; at++)
		{
			var (a, b) = (Points[at - 1], Points[at]);
			var length = Vector3.Distance(a, b);
			if (left <= length && length > 0f)
			{
				return (Vector3.Lerp(a, b, left / length), (b - a) / length);
			}
			left -= length;
		}
		var heading = Points.Count > 1 ? Vector3.Normalize(Points[^1] - Points[^2]) : Vector3.UnitZ;
		return (Points[^1], heading);
	}
}
