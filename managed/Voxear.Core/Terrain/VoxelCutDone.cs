using System;

namespace Voxear;

/// <summary>
/// What a cut did (<see cref="Voxels.Cut(System.Numerics.Vector3, float, VoxelPieces, VoxelPieceRest?)"/>,
/// <see cref="Voxels.Slice"/>): how many endless Voxel Volumes it reached, how many voxel columns of grass, cover and
/// crops it cut, how many voxels of logs and leaves it cut through, and the pieces of trees that came loose. Grass
/// alone leaves <see cref="Felled"/> 0; a tree cut through its trunk is one piece, a branch cut off another.
/// </summary>
public readonly record struct VoxelCutDone(int Volumes, int Columns, int Voxels, VoxelFelledPiece[]? Pieces)
{
	/// <summary>How many pieces of trees came loose.</summary>
	public int Felled => Pieces?.Length ?? 0;

	/// <summary>The pieces that came loose, none for a cut that felled nothing.</summary>
	public ReadOnlySpan<VoxelFelledPiece> All => Pieces;
}
