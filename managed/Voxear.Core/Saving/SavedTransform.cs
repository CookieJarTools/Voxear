namespace Voxear;

/// <summary>
/// Keeps where its Obj is in a save of the scene: its position, rotation and scale, relative to its parent. Add it
/// to an Obj in a scene that the player can move, such as a crate to push. Objs scripts make from prefabs need
/// none: a save of the scene keeps where each one is by itself.
/// </summary>
public sealed class SavedTransform : Behaviour
{
	[Saved]
	private Transform Place
	{
		get { return Obj.Transform; }
		set { Obj.Transform = value; }
	}
}
