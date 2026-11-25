using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The level's endless Voxel Volumes, ground of voxels a few centimetres across: where a ray meets it, and balls
/// carved out of it and filled with ground. Each change reaches every endless Voxel Volume the ball falls in, which makes its ground there again
/// within a frame or two, and a save of the scene keeps it, after what was changed before.
/// <code>
/// // A crater where a shell lands, and a heap of rock where a wall is raised.
/// Voxels.Carve(landed, 1.5f);
/// Voxels.Fill(wall, 1f, VoxelMaterial.Rock);
/// </code>
/// A later change goes over an earlier one: a ball carved into rock just filled takes a bite out of it.
/// </summary>
public static unsafe class Voxels
{
	/// <summary>
	/// Carves a ball of <paramref name="radius"/> metres round <paramref name="middle"/> out of every endless Voxel
	/// Volume it falls in: its ground, grass, flowers and trees. Gives how many volumes it reached, 0 without the
	/// engine.
	/// </summary>
	public static int Carve(Vector3 middle, float radius)
	{
		return Edit(middle, radius, -1);
	}

	/// <summary>
	/// Fills a ball of <paramref name="radius"/> metres round <paramref name="middle"/> with
	/// <paramref name="material"/> in every endless Voxel Volume it falls in. Gives how many volumes it reached, 0
	/// without the engine.
	/// </summary>
	public static int Fill(Vector3 middle, float radius, VoxelMaterial material)
	{
		return Edit(middle, radius, (int)material);
	}

	/// <summary>
	/// Where a ray from <paramref name="origin"/> along <paramref name="direction"/> first meets the ground of an
	/// endless Voxel Volume within <paramref name="maxDistance"/> metres: its ground as it is made and as it was changed,
	/// not its grass, flowers or trees. Every machine finds the same, so a game can carve where a player aims.
	/// <code>
	/// // Dig where the player looks.
	/// if (Voxels.Raycast(camera.Position, camera.Forward, out var hit, 6f) &amp;&amp; Input.Pressed(dig))
	/// {
	///     Voxels.Carve(hit.Point, 0.5f);
	/// }
	/// </code>
	/// </summary>
	public static bool Raycast(Vector3 origin, Vector3 direction, out VoxelHit hit, float maxDistance = 1000f)
	{
		hit = default;
		if (!Native.IsConnected)
		{
			return false;
		}
		var found = stackalloc float[7];
		ulong obj = 0;
		if (Native.Api->VoxelRaycast(&origin, &direction, maxDistance, found, &obj) != 1)
		{
			return false;
		}
		var point = new Vector3(found[0], found[1], found[2]);
		var normal = new Vector3(found[3], found[4], found[5]);
		hit = new VoxelHit(point, normal, found[6], new Obj(obj));
		return true;
	}

	private static int Edit(Vector3 middle, float radius, int fill)
	{
		if (!Native.IsConnected)
		{
			return 0;
		}
		return Math.Max(Native.Api->VoxelEdit(&middle, radius, fill), 0);
	}
}
