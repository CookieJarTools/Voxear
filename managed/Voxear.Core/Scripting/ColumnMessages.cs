using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear.Scripting;

/// <summary>Tells scripts of the columns of worlds of blocks without end made and let go, as Blocks.ColumnMade and Blocks.ColumnLetGo.</summary>
internal static class ColumnMessages
{
	public static void Dispatch(ReadOnlySpan<ColumnEventData> events)
	{
		foreach (var told in events)
		{
			var column = new BlockColumn(new Obj(told.World), told.X, told.Z, told.Size, new Vector2(told.CornerX, told.CornerZ));
			Blocks.Raise(column, told.Made == 1);
		}
	}
}
