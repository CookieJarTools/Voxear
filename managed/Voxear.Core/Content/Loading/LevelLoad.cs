using System;
using System.Collections.Generic;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// One level change, as it happens, from Level.LoadAsync: how far along it is, what went wrong, and the scenes it
/// brought in. Await it to wait for the whole thing, or, when it is held, await Ready for the moment everything is
/// read in and waiting, then call Activate once the loading screen is done with.
/// <code>
/// var switching = Level.LoadAsync("Levels/Night.level", LevelLoadOptions.Held);
/// await switching.Ready;
/// await FadeOut();
/// switching.Activate();
/// await switching;
/// </code>
/// </summary>
public sealed class LevelLoad : AsyncLoad
{
	private readonly List<Action> whenReady = new List<Action>();
	private bool allowActivation = true;
	private bool reachedReady;
	private string[] scenes = Array.Empty<string>();

	internal LevelLoad(int id, string level, LevelLoadOptions options, string? leaving) : base(id)
	{
		Level = level;
		Options = options;
		Leaving = leaving;
		allowActivation = !options.Hold;
	}

	/// <summary>The level coming in.</summary>
	public string Level { get; }

	public LevelLoadOptions Options { get; }

	/// <summary>False holds the change at IsReady; setting it back to true lets it happen at the end of the frame.</summary>
	public bool AllowActivation
	{
		get { return allowActivation; }
		set
		{
			allowActivation = value;
			Native.AllowActivation(Id, value);
		}
	}

	/// <summary>Everything is read in, and the change waits to be let in. Progress stops at 0.9 meanwhile, as in Unity.</summary>
	public bool IsReady { get; private set; }

	/// <summary>The scenes the change brought in, once it is done. Those the levels shared were kept, not brought in.</summary>
	public IReadOnlyList<string> Scenes => scenes;

	/// <summary>Await this to wait for everything to be read in and held, ready to be let in.</summary>
	public LevelReady Ready => new LevelReady(this);

	internal string? Leaving { get; }

	/// <summary>Lets the level in, when the change was asked to hold. Does nothing otherwise.</summary>
	public void Activate()
	{
		AllowActivation = true;
	}

	public LevelLoadAwaiter GetAwaiter()
	{
		return new LevelLoadAwaiter(this);
	}

	internal void WhenReady(Action continuation)
	{
		if (IsReady || IsDone)
		{
			continuation();
			return;
		}

		whenReady.Add(continuation);
	}

	internal override void Read(LoadProgress status)
	{
		IsReady = status.State == LoadProgress.Ready;
		if (status.State == LoadProgress.Loading)
		{
			Voxear.Level.Progressed(this);
		}

		if (IsReady)
		{
			ReachReady();
		}
	}

	internal override string Failure()
	{
		return Native.LevelInfo(Native.LevelInfoError, Id) ?? base.Failure();
	}

	/// <summary>Tells everyone listening how the change ended, before anything awaiting it carries on.</summary>
	internal override void Finished()
	{
		ReachReady();
		if (Error != null)
		{
			Voxear.Level.Failed(this);
			return;
		}

		var brought = Native.LevelInfo(Native.LevelInfoScenes, Id) ?? string.Empty;
		scenes = brought.Split('\n', StringSplitOptions.RemoveEmptyEntries);
		Voxear.Level.Finished(this);
	}

	private void ReachReady()
	{
		if (reachedReady)
		{
			return;
		}

		reachedReady = true;
		if (IsReady && Options.Hold)
		{
			Voxear.Level.WentReady(this);
		}

		var waiting = whenReady.ToArray();
		whenReady.Clear();
		foreach (var continuation in waiting)
		{
			continuation();
		}
	}
}
