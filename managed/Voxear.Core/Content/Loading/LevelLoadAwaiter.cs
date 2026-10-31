using System;
using System.Runtime.CompilerServices;

namespace Voxear
{
	/// <summary>Lets a LevelLoad be awaited: carries on once the new level is in, or throws LevelLoadException if it failed.</summary>
	public readonly struct LevelLoadAwaiter : INotifyCompletion
	{
		private readonly LevelLoad load;

		internal LevelLoadAwaiter(LevelLoad load)
		{
			this.load = load;
		}

		public bool IsCompleted => load.IsDone;

		public void OnCompleted(Action continuation)
		{
			load.OnCompleted(continuation);
		}

		public void GetResult()
		{
			if (load.Error != null)
			{
				throw new LevelLoadException(load.Level, load.Error);
			}
		}
	}
}
