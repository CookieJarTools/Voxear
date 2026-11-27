using System;

namespace Voxear;

/// <summary>The kinds of request players may make of the server's endless Voxel Volumes, for <see cref="VoxelRequests.Allowed"/>.</summary>
[Flags]
public enum VoxelRequestKinds
{
	None = 0,
	Dig = 1 << VoxelRequestKind.Dig,
	Place = 1 << VoxelRequestKind.Place,
	Restore = 1 << VoxelRequestKind.Restore,
	Carve = 1 << VoxelRequestKind.Carve,
	Fill = 1 << VoxelRequestKind.Fill,
	Print = 1 << VoxelRequestKind.Print,
	Cut = 1 << VoxelRequestKind.Cut,
	Blast = 1 << VoxelRequestKind.Blast,
	Hang = 1 << VoxelRequestKind.Hang,
	CutRope = 1 << VoxelRequestKind.CutRope,
	/// <summary>Digging and building, a voxel at a time or in shapes and prints.</summary>
	Building = Dig | Place | Restore | Carve | Fill | Print,
	/// <summary>Ropes tied and cut.</summary>
	Ropes = Hang | CutRope,
	All = Building | Cut | Blast | Ropes,
}
