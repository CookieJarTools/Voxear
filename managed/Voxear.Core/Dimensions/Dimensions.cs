using System;
using System.Collections.Generic;
using System.Text.Json;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The dimensions the game has made, and making more: the full way in, for a script that keeps a dimension made
/// while nobody is in it (a town's simulation, a server's mobs) or makes one ahead of the player.
/// <code>
/// var arena = await Dimensions.Make(arenaAsset, key: (ulong)match.Number);
/// arena.KeepLoaded = true;
/// ...
/// arena.LetGo();
/// </code>
/// The easy way needs none of it: Portals make the dimensions they lead into as a player comes near, and
/// <c>obj.MoveTo</c> makes the one it moves into.
/// </summary>
public static class Dimensions
{
	private static readonly List<Func<bool>> waiting = [];

	/// <summary>Every dimension made now, the level's first.</summary>
	public static IReadOnlyList<DimensionInstance> Made
	{
		get
		{
			var json = NativeDimensions.Text(NativeDimensions.MadeText, 0);
			if (json == null)
			{
				return [DimensionInstance.Level];
			}
			using var document = JsonDocument.Parse(json);
			var made = new List<DimensionInstance>();
			foreach (var handle in document.RootElement.EnumerateArray())
			{
				made.Add(new DimensionInstance(handle.GetUInt32()));
			}
			return made;
		}
	}

	/// <summary>
	/// Makes the dimension <paramref name="dimension"/> makes as <paramref name="key"/> (0 for one of which there
	/// is only one), or gives the one made already. Await it for the dimension. It stays made until it is let go
	/// where <paramref name="keepLoaded"/>; otherwise it goes once nothing has needed it for its file's Keep Loaded
	/// seconds. What was changed in it before it was last let go is there again.
	/// </summary>
	public static DimensionMake Make(Dimension dimension, ulong key = 0, bool keepLoaded = true)
	{
		var guid = dimension.Guid ?? string.Empty;
		var keep = keepLoaded ? "true" : "false";
		return new DimensionMake($$"""{"asset":"{{guid}}","key":{{key}},"keep":{{keep}}}""");
	}

	/// <summary>Makes a dungeon's own dimension, from its site as the engine reads it.</summary>
	internal static DimensionMake MakeDungeon(string site, bool keepLoaded)
	{
		var keep = keepLoaded ? "true" : "false";
		return new DimensionMake($$"""{"dungeon":{{site}},"keep":{{keep}}}""");
	}

	/// <summary>Polls <paramref name="poll"/> once a frame until it says it is over.</summary>
	internal static void Wait(Func<bool> poll)
	{
		waiting.Add(poll);
	}

	/// <summary>Once a frame: tells of a change of dimension, and moves on what waits on the engine's dimensions.</summary>
	internal static void Dispatch()
	{
		Dimension.Dispatch();
		if (waiting.Count == 0)
		{
			return;
		}
		var polls = waiting.ToArray();
		waiting.Clear();
		foreach (var poll in polls)
		{
			if (!poll())
			{
				waiting.Add(poll);
			}
		}
	}
}
