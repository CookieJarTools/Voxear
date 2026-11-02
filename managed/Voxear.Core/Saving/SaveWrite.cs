using System;
using System.Collections.Generic;
using Voxear.Saving;

namespace Voxear;

/// <summary>
/// A save being written to disk while the game carries on, from Save.WriteAsync or SaveFile.WriteAsync. Await it
/// to carry on once the save is safely written: the code after the await runs on the main thread, at the start of
/// a frame, and the await throws a SaveException when the write failed. A failure is logged as well, for a write
/// nobody awaits.
/// </summary>
public sealed class SaveWrite
{
	private static readonly List<SaveWrite> pending = new List<SaveWrite>();

	private readonly List<Action> continuations = new List<Action>();
	private readonly int ticket;

	private SaveWrite(string slot, int ticket, SaveException? failure)
	{
		Slot = slot;
		this.ticket = ticket;
		Failure = failure;
		IsDone = failure != null;
	}

	/// <summary>The slot being written.</summary>
	public string Slot { get; }

	public bool IsDone { get; private set; }

	/// <summary>Why the write failed, or null.</summary>
	public string? Error => Failure?.Message;

	internal SaveException? Failure { get; private set; }

	public SaveWriteAwaiter GetAwaiter()
	{
		return new SaveWriteAwaiter(this);
	}

	internal static SaveWrite Start(string slot, byte[] info, byte[] data)
	{
		var store = SaveStore.Current;
		SaveWrite write;
		try
		{
			write = new SaveWrite(slot, store.Write(slot, info, data), null);
		}
		catch (SaveException exception)
		{
			Log.Error($"Saving \"{slot}\" failed: {exception.Message}");
			return new SaveWrite(slot, -1, exception);
		}
		if (!write.Poll(store))
		{
			pending.Add(write);
		}
		return write;
	}

	/// <summary>Finished writes resume their awaits, once a frame.</summary>
	internal static void Update()
	{
		if (pending.Count == 0)
		{
			return;
		}

		var store = SaveStore.Current;
		var current = pending.ToArray();
		pending.Clear();
		foreach (var write in current)
		{
			if (!write.Poll(store))
			{
				pending.Add(write);
			}
		}
	}

	internal void OnCompleted(Action continuation)
	{
		if (IsDone)
		{
			continuation();
			return;
		}
		continuations.Add(continuation);
	}

	/// <summary>Asks how the write went, and returns true once it has finished.</summary>
	private bool Poll(SaveStore store)
	{
		try
		{
			if (!store.Wait(ticket, false))
			{
				return false;
			}
		}
		catch (SaveException exception)
		{
			Failure = exception;
			Log.Error($"Saving \"{Slot}\" failed: {exception.Message}");
		}

		IsDone = true;
		var waiting = continuations.ToArray();
		continuations.Clear();
		foreach (var continuation in waiting)
		{
			continuation();
		}
		return true;
	}
}
