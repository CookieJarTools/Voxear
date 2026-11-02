using System;

namespace Voxear;

/// <summary>A scene that could not load alongside the others, thrown where it is awaited. SceneLoad.Error says the same.</summary>
public sealed class SceneLoadException : Exception
{
	public SceneLoadException(string scene, string error) : base($"Could not load {scene}: {error}")
	{
		Scene = scene;
	}

	public string Scene { get; }
}
