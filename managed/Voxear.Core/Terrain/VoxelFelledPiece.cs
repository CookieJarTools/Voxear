using System.Numerics;

namespace Voxear;

/// <summary>
/// A piece of tree a cut took loose (<see cref="VoxelCutDone.Pieces"/>, <see cref="Voxels.Piece"/>): its Obj while it
/// is a body of its own, none once taken away (<see cref="VoxelPieces.Remove"/>); where the middle of its logs is in the
/// world; its size in metres as it was cut; how many voxels of logs and of leaves it has, a whole tree's thousands, a
/// twig's a few; and whether it has fallen and come to rest.
/// </summary>
public readonly record struct VoxelFelledPiece(Obj? Obj, Vector3 Place, Vector3 Size, int Logs, int Leaves, bool Resting);
