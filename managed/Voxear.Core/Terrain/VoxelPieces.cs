namespace Voxear;

/// <summary>What becomes of the pieces of trees a cut takes loose, at once.</summary>
public enum VoxelPieces
{
	/// <summary>They tip over and fall as bodies of their own, and rest as <see cref="VoxelPieceRest"/> says. As it
	/// comes.</summary>
	Fall,

	/// <summary>They are taken away at once, the tree vanishing over its stump: for a game that puts its own logs, or
	/// nothing, where they were (<see cref="VoxelFelledPiece.Place"/>).</summary>
	Remove,
}
