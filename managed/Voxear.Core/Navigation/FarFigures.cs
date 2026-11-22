using System;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// Figures a game draws far off from its own numbers, with no Obj each: residents of a town kept by the game's own
/// society, say, beyond where they are made Objs. They are drawn as the crowd's records are, within Project Settings
/// ▸ Crowds ▸ Draw records within, the nearest 2,000 of both. Set the list again every second or so: each figure walks
/// smoothly on to where it is set, and one left out goes. The list goes when another level is opened.
/// <code>
/// var figures = residents.Where(r => r.IsFar).Select(r => new FarFigure(r.Id, r.Prefab, r.Position, r.Yaw, r.Speed));
/// FarFigures.Set(figures.ToArray());
/// </code>
/// Each machine in a session sets and draws its own: nothing is sent.
/// </summary>
public static unsafe class FarFigures
{
	/// <summary>Draws these figures in place of those set before.</summary>
	public static void Set(ReadOnlySpan<FarFigure> figures)
	{
		if (!Native.IsConnected)
		{
			return;
		}

		var data = new FarFigureData[figures.Length];
		for (var at = 0; at < figures.Length; at++)
		{
			var figure = figures[at];
			data[at] = new FarFigureData
			{
				Prefab = figure.Prefab.IsValid ? figure.Prefab.EngineId : -1,
				Id = figure.Id,
				Position = figure.Position,
				Yaw = figure.Yaw,
				Speed = figure.Speed,
			};
		}
		fixed (FarFigureData* from = data)
		{
			Native.Api->FarFigures(from, data.Length);
		}
	}

	/// <summary>Draws no figures.</summary>
	public static void Clear()
	{
		if (Native.IsConnected)
		{
			Native.Api->FarFigures(null, 0);
		}
	}
}
