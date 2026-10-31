using System;
using System.Runtime.CompilerServices;

namespace Voxear
{
	/// <summary>
	/// Awaits the moment a held level change has everything read in and waits to be let in, for a loading
	/// screen to finish before LevelLoad.Activate. A change that is not held, or that failed, carries on at once.
	/// </summary>
	public readonly struct LevelReady : INotifyCompletion
	{
		private readonly LevelLoad load;

		internal LevelReady(LevelLoad load)
		{
			this.load = load;
		}

		public bool IsCompleted => load.IsReady || load.IsDone;

		public LevelReady GetAwaiter()
		{
			return this;
		}

		public void OnCompleted(Action continuation)
		{
			load.WhenReady(continuation);
		}

		public void GetResult()
		{
		}
	}
}
