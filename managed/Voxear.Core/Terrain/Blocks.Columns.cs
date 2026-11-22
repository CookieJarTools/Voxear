using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

public static unsafe partial class Blocks
{
	/// <summary>
	/// A column of a world of blocks without end was made, after the frame it was made in: its blocks are there to
	/// see and stand on. The way to wake what lives on ground that has no Obj of its own to hear
	/// <see cref="Behaviour"/>'s OnGroundLoaded.
	/// <code>
	/// Blocks.ColumnMade += column =>
	/// {
	///     foreach (var resident in town.Residents.Where(resident => column.Contains(resident.Home)))
	///     {
	///         resident.Wake();
	///     }
	/// };
	/// </code>
	/// </summary>
	public static event Action<BlockColumn>? ColumnMade;

	/// <summary>A column of a world of blocks without end was let go, after the frame it went in.</summary>
	public static event Action<BlockColumn>? ColumnLetGo;

	/// <summary>
	/// Whether the ground within <paramref name="radius"/> metres of a place is made: every world of blocks without end
	/// there has made its blocks, to see and to stand on. True where no world is being made, as a world of its blocks
	/// alone is whole from the start. A Nav Agent can be sent from a place once <see cref="Navigation.IsReady"/> says
	/// so too, a few frames later.
	/// </summary>
	public static bool IsMade(Vector3 position, float radius = 0f)
	{
		return Native.IsConnected && Native.Api->GroundMade(&position, radius) == 1;
	}

	internal static void Raise(BlockColumn column, bool made)
	{
		try
		{
			(made ? ColumnMade : ColumnLetGo)?.Invoke(column);
		}
		catch (Exception exception)
		{
			Log.Exception(exception);
		}
	}
}
