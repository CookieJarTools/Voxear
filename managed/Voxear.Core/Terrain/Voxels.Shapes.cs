using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

public static unsafe partial class Voxels
{
	private const int BoxShape = 1;
	private const int RampShape = 2;
	private const int AreaShape = 3;
	private const int CoverShape = 4;
	private const int ShapeNumbers = 14;

	/// <summary>
	/// Fills a box <paramref name="size"/> metres (along its own X, Y and Z) with <paramref name="material"/>, the middle
	/// of its foot at <paramref name="foot"/>, turned <paramref name="turn"/> degrees about the up axis, clearing
	/// <paramref name="clear"/> metres of air over its top: a paved square, a quay's edge, a levelled plot. Gives how many
	/// volumes it reached, 0 without the engine.
	/// </summary>
	public static int Box(
		Vector3 foot,
		Vector3 size,
		VoxelMaterial material,
		float turn = 0f,
		float clear = 0f,
		Color? tint = null)
	{
		return Shape(BoxShape, foot, size, turn, 0f, 0f, 0f, clear, [], (int)material, tint);
	}

	/// <summary>Carves a box out, as <see cref="Box"/> places one: a cellar, a prison cell, a cutting.</summary>
	public static int CarveBox(Vector3 foot, Vector3 size, float turn = 0f)
	{
		return Shape(BoxShape, foot, size, turn, 0f, 0f, 0f, 0f, [], -1, null);
	}

	/// <summary>
	/// A slab half a metre tall, <paramref name="width"/> by <paramref name="depth"/> metres along X and Z, the middle of
	/// its foot at <paramref name="foot"/>: a step, or half a block of ground.
	/// </summary>
	public static int Slab(Vector3 foot, float width, float depth, VoxelMaterial material, Color? tint = null)
	{
		return Box(foot, new Vector3(width, 0.5f, depth), material, 0f, 0f, tint);
	}

	/// <summary>
	/// Fills a bank, a box whose top slopes from its foot at its back (its own -Z) up to its height at its front, placed
	/// and turned as <see cref="Box"/> places one: a graded bank, a ramp down to the quay.
	/// </summary>
	public static int Ramp(
		Vector3 foot,
		Vector3 size,
		VoxelMaterial material,
		float turn = 0f,
		float clear = 0f,
		Color? tint = null)
	{
		return Shape(RampShape, foot, size, turn, 0f, 0f, 0f, clear, [], (int)material, tint);
	}

	/// <summary>
	/// Fills the outline <paramref name="outline"/> (its points along X and Z: a Vector2's Y is the world's Z) from
	/// <paramref name="low"/> up to <paramref name="high"/> metres with <paramref name="material"/>, clearing
	/// <paramref name="clear"/> metres of air over it: a street, a square, a quay, however it bends.
	/// </summary>
	public static int Area(
		ReadOnlySpan<Vector2> outline,
		float low,
		float high,
		VoxelMaterial material,
		float clear = 0f,
		Color? tint = null)
	{
		return Shape(AreaShape, Vector3.Zero, Vector3.One, 0f, low, high, 0f, clear, outline, (int)material, tint);
	}

	/// <summary>Carves the outline out from <paramref name="low"/> up to <paramref name="high"/> metres.</summary>
	public static int CarveArea(ReadOnlySpan<Vector2> outline, float low, float high)
	{
		return Shape(AreaShape, Vector3.Zero, Vector3.One, 0f, low, high, 0f, 0f, outline, -1, null);
	}

	/// <summary>
	/// Lays the top <paramref name="depth"/> metres of the ground within <paramref name="outline"/> with
	/// <paramref name="material"/>, wherever the ground is: a road paved over hills, a field laid. Clears
	/// <paramref name="clear"/> metres over it, its grass and flowers too.
	/// </summary>
	public static int Cover(
		ReadOnlySpan<Vector2> outline,
		float depth,
		VoxelMaterial material,
		float clear = 0f,
		Color? tint = null)
	{
		return Shape(CoverShape, Vector3.Zero, Vector3.One, 0f, 0f, 0f, depth, clear, outline, (int)material, tint);
	}

	private static int Shape(
		int shape,
		Vector3 at,
		Vector3 size,
		float turn,
		float low,
		float high,
		float depth,
		float clear,
		ReadOnlySpan<Vector2> outline,
		int fill,
		Color? tint)
	{
		if (!Native.IsConnected)
		{
			return 0;
		}
		var count = ShapeNumbers + outline.Length * 2;
		var numbers = count <= 256 ? stackalloc float[count] : new float[count];
		numbers.Clear();
		numbers[0] = at.X;
		numbers[1] = at.Y;
		numbers[2] = at.Z;
		numbers[3] = size.X;
		numbers[4] = size.Y;
		numbers[5] = size.Z;
		numbers[7] = turn;
		numbers[9] = low;
		numbers[10] = high;
		numbers[11] = depth;
		numbers[12] = clear;
		numbers[13] = 1f;
		for (var i = 0; i < outline.Length; i++)
		{
			numbers[ShapeNumbers + i * 2] = outline[i].X;
			numbers[ShapeNumbers + i * 2 + 1] = outline[i].Y;
		}
		fixed (float* sent = numbers)
		{
			return Math.Max(Native.Api->VoxelShape(shape, sent, count, fill, Tint(tint)), 0);
		}
	}

	/// <summary>A tint as the engine takes it: 0 for none, else 1 over its red, green and blue bytes.</summary>
	private static uint Tint(Color? tint)
	{
		if (tint is not { } colour)
		{
			return 0u;
		}
		static uint Byte(float part) => (uint)Math.Clamp(MathF.Round(part * 255f), 0f, 255f);
		return 1u << 24 | Byte(colour.R) << 16 | Byte(colour.G) << 8 | Byte(colour.B);
	}
}
