using System;
using System.Collections.Generic;
using Voxear.Interop;
using Voxear.Scripting;

namespace Voxear;

/// <summary>
/// A load running in the background while the game carries on. Read Progress for a loading bar, or await it: the
/// code after the await runs on the main thread, at the start of the frame after the load finishes.
/// </summary>
public abstract class AsyncLoad
{
	private readonly List<Action> continuations = new List<Action>();

	internal AsyncLoad(int id)
	{
		Id = id;
		if (id < 0)
		{
			// Too early to tell anyone: the derived load is not built yet, so whoever started it reports it.
			Error = "It could not start; the log says why.";
			IsDone = true;
		}
	}

	/// <summary>From 0 to 1.</summary>
	public float Progress { get; private set; }

	public bool IsDone { get; private set; }

	/// <summary>Why the load failed, or null. The log has the details.</summary>
	public string? Error { get; private set; }

	internal int Id { get; }

	internal void OnCompleted(Action continuation)
	{
		if (IsDone)
		{
			continuation();
			return;
		}
		continuations.Add(continuation);
	}

	/// <summary>Reads the engine's status, and returns true once the load has finished.</summary>
	internal bool Poll()
	{
		if (!Native.LoadStatus(Id, out var status))
		{
			Finish("The engine no longer knows this load.");
			return true;
		}

		Progress = status.Progress;
		Read(status);
		if (status.State == LoadProgress.Done)
		{
			Finish(null);
		}
		else if (status.State == LoadProgress.Failed)
		{
			Finish(Failure());
		}
		return IsDone;
	}

	/// <summary>Takes what a finished or waiting load reports, such as the loaded prefab.</summary>
	internal virtual void Read(LoadProgress status)
	{
	}

	/// <summary>What Error says when the load failed.</summary>
	internal virtual string Failure()
	{
		return "Loading failed; the log says why.";
	}

	/// <summary>Called once the load is over, before anything awaiting it carries on.</summary>
	internal virtual void Finished()
	{
	}

	private void Finish(string? error)
	{
		Error = error;
		IsDone = true;
		Progress = error == null ? 1f : Progress;
		Finished();
		var waiting = continuations.ToArray();
		continuations.Clear();
		foreach (var continuation in waiting)
		{
			continuation();
		}
	}
}
