using System.Numerics;
using Voxear.Interop;

namespace Voxear;

public static unsafe partial class Voxels
{
	/// <summary>
	/// Ties a rope at <paramref name="tied"/> holding the point <paramref name="holds"/> of what was built in an endless
	/// Voxel Volume: an Obj of its own with a Voxel Rope. What is built round that point and held up by nothing else
	/// hangs from it at once, and so does whatever comes loose later with that point in it: it swings. The rope is
	/// <paramref name="length"/> metres long, or as far as between the two where 0, and snaps when what hangs from it
	/// pulls harder than <paramref name="strength"/> newtons, never where 0, or when a blast reaches it. Gives the
	/// rope's Obj, none without the engine.
	/// <code>
	/// // A cage hung from the cliff's overhang: blow the overhang's foot and it swings out over the drop.
	/// var rope = Voxels.Hang(overhang + Vector3.UnitY * 0.2f, cage.Top, strength: 30000f);
	/// // ...later, a bolt through the rope:
	/// if (rope is { } tied) Voxels.CutRope(tied);
	/// </code>
	/// </summary>
	public static Obj? Hang(Vector3 tied, Vector3 holds, float length = 0f, float strength = 0f)
	{
		if (!Native.IsConnected)
		{
			return null;
		}
		var numbers = stackalloc float[8] { tied.X, tied.Y, tied.Z, holds.X, holds.Y, holds.Z, length, strength };
		var obj = Native.Api->VoxelRope(0, numbers, 0);
		return obj == 0 ? null : new Obj(obj);
	}

	/// <summary>Cuts rope <paramref name="rope"/>, an Obj <see cref="Hang"/> gave or one with a Voxel Rope: what hangs from
	/// it falls. False for an Obj with no rope, or one cut already.</summary>
	public static bool CutRope(Obj rope)
	{
		return Native.IsConnected && Native.Api->VoxelRope(1, null, rope.Id) != 0;
	}
}
