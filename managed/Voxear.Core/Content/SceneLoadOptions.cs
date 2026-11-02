namespace Voxear;

/// <summary>
/// How Scene.LoadAsync brings a scene in: as the active scene or not, again when it is loaded already, and straight
/// in or held for a loading screen.
/// </summary>
public sealed class SceneLoadOptions
{
	/// <summary>Whether it becomes the active scene once it is in: where Objs made at runtime go, and whose sky is used. Off by default.</summary>
	public bool SetActive { get; set; }

	/// <summary>Whether it loads again when it is loaded already, its Objs made afresh. Off by default, when a loaded scene stays as it is.</summary>
	public bool Reload { get; set; }

	/// <summary>Whether it waits at IsReady once everything is read in, until SceneLoad.Activate is called. Off by default.</summary>
	public bool Hold { get; set; }
}
