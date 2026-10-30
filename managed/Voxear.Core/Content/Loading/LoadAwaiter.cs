using System;
using System.Runtime.CompilerServices;

namespace Voxear
{
	/// <summary>Lets a LevelLoad be awaited. Check the load's Error afterwards to see whether it worked.</summary>
	public readonly struct LoadAwaiter : INotifyCompletion
	{
		private readonly AsyncLoad load;

		internal LoadAwaiter(AsyncLoad load)
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
		}
	}
}
