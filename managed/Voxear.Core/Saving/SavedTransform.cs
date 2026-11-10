namespace Voxear;

/// <summary>
/// Keeps where its Obj is in a save of the scene: its position, rotation and scale, relative to its parent, or with
/// none from the level's real middle, wherever the world's has moved to (see Origin). Add it to an Obj in a scene
/// that the player can move, such as a crate to push. Objs scripts make from prefabs need none: a save of the scene
/// keeps where each one is by itself.
/// </summary>
public sealed class SavedTransform : Behaviour
{
	[Saved]
	private Transform Place
	{
		get { return Obj.Parent.IsValid ? Obj.Transform : Offset(Obj.Transform, Origin.Position, 1); }
		set { Obj.Transform = Obj.Parent.IsValid ? value : Offset(value, Origin.Position, -1); }
	}

	/// <summary>A root's place measured from the level's real middle (way 1), or back from there (way -1).</summary>
	private static Transform Offset(Transform place, LevelPosition origin, int way)
	{
		place.Position += new System.Numerics.Vector3((float)origin.X, (float)origin.Y, (float)origin.Z) * way;
		return place;
	}
}
