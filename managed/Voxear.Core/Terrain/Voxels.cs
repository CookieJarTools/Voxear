using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The level's endless Voxel Volumes, ground of voxels a few centimetres across: balls carved out of it and filled
/// with ground. Each change reaches every endless Voxel Volume the ball falls in, which makes its ground there again
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

	private static int Edit(Vector3 middle, float radius, int fill)
	{
		if (!Native.IsConnected)
		{
			return 0;
		}
		return Math.Max(Native.Api->VoxelEdit(&middle, radius, fill), 0);
	}
}
