using System;
using System.Collections.Generic;
using System.Runtime.CompilerServices;
using System.Text.Json;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A dimension being made, from Dimensions.Make: await it for the dimension. The game makes it once scripts have
/// run this frame, and the code after the await runs the frame after. One made already is there at once.
/// <code>
/// var caves = await Dimensions.Make(cavesAsset);
/// await caves.WhenReady(spawn);
/// </code>
/// </summary>
public sealed class DimensionMake : INotifyCompletion
{
	private readonly List<Action> continuations = [];
	private readonly int ask;

	internal DimensionMake(string asked)
	{
		var answer = NativeDimensions.Ask(NativeDimensions.MakeText, 0, asked);
		if (answer == null)
		{
			Finish(null, "The engine could not make it: no such dimension, or no engine.");
			return;
		}
		using var document = JsonDocument.Parse(answer);
		var root = document.RootElement;
		if (root.TryGetProperty("dimension", out var made))
		{
			Finish(new DimensionInstance(made.GetUInt32()), null);
			return;
		}
		ask = root.GetProperty("ask").GetInt32();
		Dimensions.Wait(Poll);
	}

	/// <summary>Whether it is made, or could not be.</summary>
	public bool IsDone { get; private set; }

	/// <summary>Why it could not be made, or null. The log has the details.</summary>
	public string? Error { get; private set; }

	/// <summary>The dimension once it is made: the level's until then, and where it could not be.</summary>
	public DimensionInstance Dimension { get; private set; }

	/// <summary>Whether a dimension was made.</summary>
	public bool IsMade => IsDone && Error == null;

	public bool IsCompleted => IsDone;

	public DimensionMake GetAwaiter()
	{
		return this;
	}

	public void OnCompleted(Action continuation)
	{
		if (IsDone)
		{
			continuation();
			return;
		}
		continuations.Add(continuation);
	}

	public DimensionInstance GetResult()
	{
		return Dimension;
	}

	/// <summary>Reads how the ask went, and returns true once it is over.</summary>
	private bool Poll()
	{
		var answer = NativeDimensions.Ask(NativeDimensions.AskedText, 0, ask.ToString());
		if (answer == null)
		{
			Finish(null, "The engine no longer knows this ask.");
			return true;
		}
		using var document = JsonDocument.Parse(answer);
		var root = document.RootElement;
		if (root.TryGetProperty("dimension", out var made))
		{
			Finish(new DimensionInstance(made.GetUInt32()), null);
		}
		else if (root.TryGetProperty("error", out var error))
		{
			Finish(null, error.GetString());
		}
		return IsDone;
	}

	private void Finish(DimensionInstance? made, string? error)
	{
		(Dimension, Error, IsDone) = (made ?? DimensionInstance.Level, error, true);
		if (error != null)
		{
			Log.Error($"A dimension could not be made: {error}");
		}
		var waiting = continuations.ToArray();
		continuations.Clear();
		foreach (var continuation in waiting)
		{
			continuation();
		}
	}
}
