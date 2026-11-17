using System;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// While it lasts, every call that takes a place asks one dimension, as though the game were seen from there. Made
/// by <see cref="DimensionInstance.Use"/>; dispose it, with <c>using</c>, to ask the dimension asked before.
/// </summary>
public readonly struct DimensionScope : IDisposable
{
	private readonly bool asking;

	internal DimensionScope(DimensionInstance dimension)
	{
		asking = NativeDimensions.AskIn(dimension.Handle);
		if (!asking)
		{
			Log.Warning($"{dimension} is not made, so the calls in its scope ask the dimension the game is seen from.");
		}
	}

	public void Dispose()
	{
		if (asking)
		{
			NativeDimensions.AskEnd();
		}
	}
}
