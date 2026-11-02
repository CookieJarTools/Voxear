using System;
using System.Runtime.CompilerServices;

namespace Voxear;

/// <summary>Lets a SceneLoad be awaited: carries on once the scene is in, or throws SceneLoadException if it failed.</summary>
public readonly struct SceneLoadAwaiter : INotifyCompletion
{
	private readonly SceneLoad load;

	internal SceneLoadAwaiter(SceneLoad load)
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
			throw new SceneLoadException(load.Scene, load.Error);
		}
	}
}
