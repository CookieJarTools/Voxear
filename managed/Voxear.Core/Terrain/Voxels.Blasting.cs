using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

public static unsafe partial class Voxels
{
	/// <summary>
	/// Blows out what was built in a ball of <paramref name="radius"/> metres round <paramref name="middle"/> in every
	/// endless Voxel Volume it reaches: every voxel an edit filled, a print printed or a game placed is dug out at once,
	/// dust flying, and the ground is left. What that leaves cut off from what holds it up comes loose as soon as the
	/// volume has worked it out, a moment later, thrown away from the blast at <paramref name="throw"/> metres a
	/// second; what is left overloaded gives way after, a round at a time, a wall over a breach, a roof over a blown
	/// corner. The pieces fall as bodies of their own, or are taken away as <paramref name="pieces"/> says, and are
	/// stamped back into the volume where they come to rest. Digging (<see cref="Dig(Vector3)"/>) and carving
	/// (<see cref="Carve"/>) let what they cut off come loose too.
	/// <code>
	/// // A shell lands: the wall it hits is blown out, and the house round it sags and falls in.
	/// var done = Voxels.Blast(landed, 1.2f);
	/// if (done.Dug > 0) Audio.Play(rubble, landed);
	/// </code>
	/// </summary>
	public static VoxelBlastDone Blast(Vector3 middle, float radius, VoxelPieces pieces = VoxelPieces.Fall, float @throw = 3f)
	{
		if (!Native.IsConnected)
		{
			return default;
		}
		var done = stackalloc int[2];
		var reached = Native.Api->VoxelBlast(&middle, radius, (int)pieces, @throw, done);
		return reached < 0 ? default : new VoxelBlastDone(done[0], done[1]);
	}
}
