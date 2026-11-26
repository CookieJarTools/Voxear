using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

public static unsafe partial class Voxels
{
	/// <summary>
	/// Digs out the one voxel at <paramref name="point"/> in every endless Voxel Volume it falls in, over everything its
	/// edits make: a pick's blow. Kept with the volume and saved in region files beside its scene. Gives how many
	/// voxels were dug, 0 without the engine.
	/// <code>
	/// // Dig the voxel the player looks at, and build one on the face beside it.
	/// if (Voxels.Raycast(camera.Position, camera.Forward, out var hit, 6f))
	/// {
	///     if (Input.Pressed(dig)) Voxels.Dig(hit.Point - hit.Normal * 0.01f);
	///     if (Input.Pressed(build)) Voxels.Place(hit.Point + hit.Normal * 0.01f, VoxelMaterial.Planks);
	/// }
	/// </code>
	/// </summary>
	public static int Dig(Vector3 point)
	{
		return SetVoxels([point], -1, null);
	}

	/// <summary>Digs out the voxel at each of <paramref name="points"/>, as <see cref="Dig(Vector3)"/> does one.</summary>
	public static int Dig(ReadOnlySpan<Vector3> points)
	{
		return SetVoxels(points, -1, null);
	}

	/// <summary>
	/// Places one voxel of <paramref name="material"/> at <paramref name="point"/>, in <paramref name="tint"/> where
	/// given, in every endless Voxel Volume it falls in, over everything its edits make. A volume keeps up to 255
	/// kinds of placed voxel, a ground or paving and a tint each. Gives how many voxels were placed.
	/// </summary>
	public static int Place(Vector3 point, VoxelMaterial material, Color? tint = null)
	{
		return SetVoxels([point], (int)material, tint);
	}

	/// <summary>Places a voxel at each of <paramref name="points"/>, as <see cref="Place(Vector3, VoxelMaterial, Color?)"/> does one.</summary>
	public static int Place(ReadOnlySpan<Vector3> points, VoxelMaterial material, Color? tint = null)
	{
		return SetVoxels(points, (int)material, tint);
	}

	/// <summary>
	/// Lets the voxel at each of <paramref name="points"/> go back to what its volume's ground and edits make, undoing a
	/// dig or a placing.
	/// </summary>
	public static int Restore(ReadOnlySpan<Vector3> points)
	{
		return SetVoxels(points, -2, null);
	}

	private static int SetVoxels(ReadOnlySpan<Vector3> points, int to, Color? tint)
	{
		if (!Native.IsConnected || points.IsEmpty)
		{
			return 0;
		}
		fixed (Vector3* sent = points)
		{
			return Math.Max(Native.Api->VoxelSet(sent, points.Length, to, Tint(tint)), 0);
		}
	}
}
