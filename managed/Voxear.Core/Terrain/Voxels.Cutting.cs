using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

public static unsafe partial class Voxels
{
	/// <summary>The most pieces one cut reports.</summary>
	private const int MostPieces = 64;

	/// <summary>
	/// Cuts everything in a ball of <paramref name="radius"/> metres round <paramref name="middle"/> in every endless
	/// Voxel Volume it reaches: the grass, cover and crops of the columns it reaches fade away and stay cut until they
	/// grow back, and the logs and leaves in it are cut out. Where that cuts through a tree's trunk or a branch, what is
	/// no longer joined to the ground by logs comes loose: it falls as a body of its own away from the cut, or is taken
	/// away, as <paramref name="pieces"/> says, and at rest stays, is stamped back or goes as <paramref name="rest"/>
	/// says, or the volume's Felled where it is left out. The ground is not cut. Kept with the volume and saved with the
	/// scene.
	/// <code>
	/// // A scythe's sweep in front of the player: grass only, or a sapling's worth of wood too.
	/// var done = Voxels.Cut(player.Position + player.Forward, 1.2f);
	/// if (done.Columns > 0) Audio.Play(swish, player.Position);
	/// </code>
	/// </summary>
	public static VoxelCutDone Cut(Vector3 middle, float radius, VoxelPieces pieces = VoxelPieces.Fall, VoxelPieceRest? rest = null)
	{
		Span<float> numbers = [middle.X, middle.Y, middle.Z, radius];
		return CutWith(1, numbers, null, pieces, rest);
	}

	/// <summary>
	/// Cuts the voxel at each of <paramref name="points"/>, and the grass of its column where the point is in it: a
	/// blade's edge drawn through them, as <see cref="Cut(Vector3, float, VoxelPieces, VoxelPieceRest?)"/> cuts a ball.
	/// </summary>
	public static VoxelCutDone Cut(ReadOnlySpan<Vector3> points, VoxelPieces pieces = VoxelPieces.Fall, VoxelPieceRest? rest = null)
	{
		if (points.IsEmpty)
		{
			return default;
		}
		var numbers = new float[points.Length * 3];
		for (var i = 0; i < points.Length; i++)
		{
			(numbers[i * 3], numbers[i * 3 + 1], numbers[i * 3 + 2]) = (points[i].X, points[i].Y, points[i].Z);
		}
		return CutWith(0, numbers, null, pieces, rest);
	}

	/// <summary>
	/// An axe's or a sword's cut: a slice through <paramref name="middle"/> square to <paramref name="across"/>, as far
	/// as <paramref name="radius"/> metres from it and a voxel thick, cutting what it passes through as
	/// <see cref="Cut(Vector3, float, VoxelPieces, VoxelPieceRest?)"/> does. A tree cut through its trunk tips over on
	/// its stump and falls the way <paramref name="fall"/> says across, or away from the cut where it is zero.
	/// <code>
	/// // Fell the tree the player looks at, away from them; once it lies still, it is part of the ground.
	/// if (Voxels.Raycast(camera.Position, camera.Forward, out var hit, 3f))
	/// {
	///     Voxels.Slice(hit.Point, Vector3.UnitY, 0.8f, fall: camera.Forward, rest: VoxelPieceRest.Stamp);
	/// }
	///
	/// // Or the tree vanishes, and the game drops logs of its own, as many as the tree had wood.
	/// var done = Voxels.Slice(hit.Point, Vector3.UnitY, 0.8f, pieces: VoxelPieces.Remove);
	/// foreach (var piece in done.All)
	/// {
	///     for (var i = 0; i &lt; piece.Logs / 4000; i++) LogPrefab.Instantiate(piece.Place + Vector3.UnitY * i, Quaternion.Identity);
	/// }
	/// </code>
	/// </summary>
	public static VoxelCutDone Slice(Vector3 middle, Vector3 across, float radius, Vector3 fall = default, VoxelPieces pieces = VoxelPieces.Fall, VoxelPieceRest? rest = null)
	{
		Span<float> numbers = [middle.X, middle.Y, middle.Z, across.X, across.Y, across.Z, radius];
		return CutWith(2, numbers, fall == Vector3.Zero ? null : fall, pieces, rest);
	}

	/// <summary>
	/// What felled piece <paramref name="obj"/> is: where it is, how large, how much wood and leaves, and whether it has
	/// come to rest. False for an Obj that is not one, or one already stamped or taken away.
	/// <code>
	/// // Once it lies still, a fallen tree is stamped into the ground, unless it fell into the river.
	/// if (Voxels.Piece(felled, out var piece) &amp;&amp; piece.Resting)
	/// {
	///     if (Voxels.WaterAt(piece.Place, out _)) Voxels.RemovePiece(felled);
	///     else Voxels.StampPiece(felled);
	/// }
	/// </code>
	/// </summary>
	public static bool Piece(Obj obj, out VoxelFelledPiece piece)
	{
		return PieceAct(obj, 0, out piece);
	}

	/// <summary>Stamps felled piece <paramref name="obj"/> into its volume where it lies now, at rest or not, and takes
	/// its Obj away. False for an Obj that is not a felled piece.</summary>
	public static bool StampPiece(Obj obj)
	{
		return PieceAct(obj, 1, out _);
	}

	/// <summary>Takes felled piece <paramref name="obj"/> away. False for an Obj that is not one.</summary>
	public static bool RemovePiece(Obj obj)
	{
		return PieceAct(obj, 2, out _);
	}

	private static bool PieceAct(Obj obj, int act, out VoxelFelledPiece piece)
	{
		piece = default;
		if (!Native.IsConnected)
		{
			return false;
		}
		var info = stackalloc float[9];
		if (Native.Api->VoxelPiece(obj.Id, act, info) != 1)
		{
			return false;
		}
		piece = PieceOf(info, obj.Id);
		return true;
	}

	private static VoxelFelledPiece PieceOf(float* n, ulong obj)
	{
		var size = new Vector3(n[3], n[4], n[5]);
		return new VoxelFelledPiece(obj == 0 ? null : new Obj(obj), new Vector3(n[0], n[1], n[2]), size, (int)n[6], (int)n[7], n[8] != 0f);
	}

	private static VoxelCutDone CutWith(int shape, ReadOnlySpan<float> numbers, Vector3? fall, VoxelPieces pieces, VoxelPieceRest? rest)
	{
		if (!Native.IsConnected)
		{
			return default;
		}
		var done = stackalloc int[3];
		var found = stackalloc float[MostPieces * 9];
		var objs = stackalloc ulong[MostPieces];
		var toward = fall.GetValueOrDefault();
		var resting = rest is { } chosen ? (int)chosen : -1;
		int reached;
		fixed (float* sent = numbers)
		{
			reached = Native.Api->VoxelCut(shape, sent, numbers.Length, fall.HasValue ? &toward : null, (int)pieces, resting, done, found, objs, MostPieces);
		}
		if (reached < 0)
		{
			return default;
		}
		var felled = new VoxelFelledPiece[Math.Min(done[2], MostPieces)];
		for (var i = 0; i < felled.Length; i++)
		{
			felled[i] = PieceOf(found + i * 9, objs[i]);
		}
		return new VoxelCutDone(reached, done[0], done[1], felled);
	}
}
