using System;
using System.Collections.Generic;
using System.Threading;

namespace Voxear.Scripting
{
	/// <summary>
	/// Brings await continuations back to the main thread, as Unity's synchronisation context does. Anything posted,
	/// from any thread, runs at the start of the next frame, so code after an await (a load, Task.Delay or a web
	/// request) can touch Objs safely.
	/// </summary>
	internal sealed class MainThreadContext : SynchronizationContext
	{
		public static readonly MainThreadContext Instance = new MainThreadContext();

		private readonly object gate = new object();
		private List<(SendOrPostCallback Callback, object? State)> queued = new List<(SendOrPostCallback, object?)>();
		private List<(SendOrPostCallback Callback, object? State)> running = new List<(SendOrPostCallback, object?)>();
		private int mainThread;

		public void Install()
		{
			mainThread = Environment.CurrentManagedThreadId;
			SetSynchronizationContext(this);
		}

		public override void Post(SendOrPostCallback callback, object? state)
		{
			lock (gate)
			{
				queued.Add((callback, state));
			}
		}

		public override void Send(SendOrPostCallback callback, object? state)
		{
			if (Environment.CurrentManagedThreadId != mainThread)
			{
				throw new NotSupportedException("Send from another thread would wait on the frame; use Post.");
			}
			callback(state);
		}

		public override SynchronizationContext CreateCopy()
		{
			return this;
		}

		/// <summary>
		/// Runs what was posted before this call. Anything posted while it runs waits for the next frame, so a
		/// continuation that keeps posting cannot hold up the frame.
		/// </summary>
		public void Run()
		{
			lock (gate)
			{
				(queued, running) = (running, queued);
			}

			foreach (var (callback, state) in running)
			{
				Invoke(callback, state);
			}
			running.Clear();
		}

		private static void Invoke(SendOrPostCallback callback, object? state)
		{
			try
			{
				callback(state);
			}
			catch (Exception exception)
			{
				Log.Exception(exception);
			}
		}
	}
}
