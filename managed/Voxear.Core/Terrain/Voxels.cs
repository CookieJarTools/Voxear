using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The level's endless Voxel Volumes, ground of voxels a few centimetres across: where a ray meets it, balls carved
/// out of it and filled with ground, Voxel Prints printed into it, the water of its rivers and lakes, and its sea. Each change reaches every endless Voxel Volume
/// it falls in, which makes its ground there again within a frame or two, and a save of the scene keeps it, after what
/// was changed before.
/// <code>
/// // A crater where a shell lands, a heap of rock where a wall is raised, and sandbags round a dug-out.
/// Voxels.Carve(landed, 1.5f);
/// Voxels.Fill(wall, 1f, VoxelMaterial.Rock);
/// Voxels.Print(SandbagWall, dugOut, turn: 90f);
/// </code>
/// A later change goes over an earlier one: a ball carved into rock just filled, or into a print, takes a bite out of
/// it.
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
	/// Prints <paramref name="print"/> into every endless Voxel Volume its box reaches, its model's origin at
	/// <paramref name="at"/>, turned <paramref name="turn"/> degrees about the up axis. It is solid all through, so a
	/// ball carved into it later finds more of it, in the colours nearest its skin. It is printed once the frame is done;
	/// gives whether it was asked for, false without the engine or a print.
	/// </summary>
	public static bool Print(VoxelPrint print, Vector3 at, float turn = 0f)
	{
		return Print(print, at, new Vector3(0f, turn, 0f));
	}

	/// <summary>
	/// Prints <paramref name="print"/> as <see cref="Print(VoxelPrint, Vector3, float)"/> does, turned
	/// <paramref name="rotation"/> degrees about X, Y and Z, as an Obj's rotation is written: leaning with a slope, say.
	/// </summary>
	public static bool Print(VoxelPrint print, Vector3 at, Vector3 rotation)
	{
		if (!Native.IsConnected || !print.IsSet)
		{
			return false;
		}
		var bytes = System.Text.Encoding.UTF8.GetBytes(print.Reference);
		fixed (byte* text = bytes)
		{
			return Native.Api->VoxelPrint(text, bytes.Length, &at, &rotation) == 1;
		}
	}

	/// <summary>
	/// Where a ray from <paramref name="origin"/> along <paramref name="direction"/> first meets the ground of an
	/// endless Voxel Volume within <paramref name="maxDistance"/> metres: its ground as it is made and as it was changed,
	/// the prints printed into it with the rest, and its trees' logs and leaves; not its grass or flowers. Every machine
	/// finds the same, so a game can carve where a player aims.
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

	/// <summary>
	/// The water of an endless Voxel Volume's rivers and lakes at <paramref name="position"/>: how high its surface
	/// stands, and which way and how fast its current runs, in metres a second, nothing on a lake. False where none has
	/// water there, more than a metre under a river's bed, or without the engine. What floats and swims in it reads the
	/// same through <see cref="Water"/>, with the sea and every other water.
	/// <code>
	/// // A fishing float rides the river downstream.
	/// if (Voxels.WaterAt(floatAt, out var river))
	/// {
	///     floatAt = floatAt with { Y = river.Height } + river.Current * Time.DeltaTime;
	/// }
	/// </code>
	/// </summary>
	public static bool WaterAt(Vector3 position, out VoxelWater water)
	{
		water = default;
		if (!Native.IsConnected)
		{
			return false;
		}
		var found = stackalloc float[4];
		ulong obj = 0;
		if (Native.Api->VoxelWater(&position, found, &obj) != 1)
		{
			return false;
		}
		water = new VoxelWater(found[0], new Vector3(found[1], found[2], found[3]), new Obj(obj));
		return true;
	}

	/// <summary>
	/// Whether the endless Voxel Volumes with a generator have its sea, filling the level at its sea level where the
	/// level has no Ocean of its own. Setting it turns every one's on or off, as their Sea in the Inspector.
	/// </summary>
	public static bool Sea
	{
		get => Native.IsConnected && SeaCall(-1, out _);
		set
		{
			if (Native.IsConnected)
			{
				SeaCall(value ? 1 : 0, out _);
			}
		}
	}

	/// <summary>
	/// How high the endless Voxel Volumes' generator's sea stands in the world, where one has its sea on; else
	/// <see cref="float.NaN"/>.
	/// </summary>
	public static float SeaLevel => Native.IsConnected && SeaCall(-1, out var level) ? level : float.NaN;

	private static bool SeaCall(int on, out float level)
	{
		float found = 0f;
		var has = Native.Api->VoxelSea(on, &found) == 1;
		level = found;
		return has;
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
