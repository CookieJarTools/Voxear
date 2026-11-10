using System;
using System.Numerics;
using System.Runtime.Loader;
using Voxear.Interop;
using Voxear.Scripting;

namespace Voxear;

/// <summary>
/// Where the world's middle is. Far from the middle a float loses its smallest steps, so in a world without end, or
/// on a planet, the engine now and then moves everything back towards the middle by whole metres, keeping the
/// camera near it. Objs, physics, sound and effects all move together, between two frames, and every position a
/// script reads is where things are now.
/// <para>
/// A field keeping a position in the world moves too when it is marked [WorldPosition]. Anything else that keeps
/// one can move it in Moved, or keep it as a LevelPosition, which the middle moving does not change.
/// </para>
/// <code>
/// [WorldPosition] Vector3 lastSeen;
///
/// Origin.Moved += by => heardAt += by;
/// var fromLevel = Origin.ToLevel(Obj.WorldPosition);
/// </code>
/// It moves only in a project that switches it on, in Project Settings ▸ World. In a networked game the server keeps
/// the level's middle and each player's machine moves its own; positions go between them from the level's middle,
/// script positions too where a [Replicated] property or an RPC argument is marked [WorldPosition].
/// </summary>
public static class Origin
{
	/// <summary>Where the world's middle is from the level's real middle, in whole metres: zero until it first moves.</summary>
	public static LevelPosition Position => Native.WorldOrigin();

	/// <summary>Whether the world's middle has moved from the level's at all.</summary>
	public static bool IsMoved => Position != default;

	/// <summary>
	/// The world just moved, by this much: every Obj and every [WorldPosition] field has moved by it already.
	/// </summary>
	public static event Action<Vector3>? Moved;

	/// <summary>A position in the world, measured from the level's real middle.</summary>
	public static LevelPosition ToLevel(Vector3 at)
	{
		return Position + at;
	}

	/// <summary>A position from the level's real middle, in the world as it is now.</summary>
	public static Vector3 FromLevel(LevelPosition at)
	{
		return at - Position;
	}

	internal static void Move(Vector3 by)
	{
		WorldPositions.Move(by);
		try
		{
			Moved?.Invoke(by);
		}
		catch (Exception exception)
		{
			Log.Exception(exception);
		}
	}

	/// <summary>Lets go of the listeners and field lists of scripts about to be reloaded.</summary>
	internal static void Forget(AssemblyLoadContext old)
	{
		Moved = GameCode.Without(Moved, old);
		WorldPositions.Forget();
	}
}
