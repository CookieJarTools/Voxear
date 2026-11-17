using System;
using System.Numerics;

namespace Voxear;

/// <summary>
/// A way found over the nav: the corners to walk straight between, the first where it begins, each on the floor;
/// whether it reaches where it was asked to go; and what it costs. Get one from Navigation.FindPath or an agent's
/// Path.
/// <code>
/// var path = Navigation.FindPath(Obj.Position, chest.Position);
/// if (path.Status == NavPathStatus.Complete)
/// {
/// 	Log.Info($"The chest is {path.Length:0} metres away on foot, round {path.Corners.Length - 2} corners.");
/// }
/// </code>
/// </summary>
public readonly struct NavPath
{
	public static readonly NavPath None = new(NavPathStatus.None, [], 0f);

	public NavPath(NavPathStatus status, Vector3[] corners, float cost)
	{
		Status = status;
		Corners = corners;
		Cost = cost;
	}

	/// <summary>Whether it reaches where it was asked to go, or only as near as it can.</summary>
	public NavPathStatus Status { get; }

	/// <summary>The corners to walk straight between, in the world.</summary>
	public Vector3[] Corners { get; }

	/// <summary>What it costs to walk, by the costs of the ground it crosses: a metre of plain ground costs 1.</summary>
	public float Cost { get; }

	/// <summary>Whether there is a way to walk at all.</summary>
	public bool Exists => Corners.Length > 0;

	/// <summary>How far it goes along its corners, in metres.</summary>
	public float Length
	{
		get
		{
			var length = 0f;
			for (var at = 1; at < Corners.Length; at++)
			{
				length += Vector3.Distance(Corners[at - 1], Corners[at]);
			}

			return length;
		}
	}

	public override string ToString()
	{
		return $"{Status} way, {Corners.Length} corners, {Length:0.#} m";
	}
}
