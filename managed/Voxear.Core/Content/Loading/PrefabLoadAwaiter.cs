using System;
using System.Runtime.CompilerServices;

namespace Voxear
{
	/// <summary>Lets a PrefabLoad be awaited for its prefab, which is None when the load failed.</summary>
	public readonly struct PrefabLoadAwaiter : INotifyCompletion
	{
		private readonly PrefabLoad load;

		internal PrefabLoadAwaiter(PrefabLoad load)
		{
			this.load = load;
		}

		public bool IsCompleted => load.IsDone;

		public void OnCompleted(Action continuation)
		{
			load.OnCompleted(continuation);
		}

		public Prefab GetResult()
		{
			return load.Prefab;
		}
	}
}
