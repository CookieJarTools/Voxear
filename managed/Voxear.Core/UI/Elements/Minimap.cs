namespace Voxear;

/// <summary>
/// A minimap in a UI of your own: the ground round what the maps follow (see Map.Follow), drawn as a small model
/// of the scene's Block World, with the markers on it. Give it a size in its style. In markup it is
/// <c>&lt;Minimap zoom="2" turns="true" /&gt;</c>.
/// <code>
/// var minimap = hud.Root.Add(new Minimap { Zoom = 4, Round = true });
/// minimap.AddClass("corner");
/// </code>
/// <para>
/// Sheets style it by its classes: <c>minimap</c> for the whole of it, <c>map-picture</c> for the ground and
/// <c>map-overlay</c> for what lies over it.
/// </para>
/// </summary>
public sealed class Minimap : MapElement
{
	public Minimap() : base(UiKind.Minimap, false)
	{
	}

	internal Minimap(int id) : base(id, false)
	{
	}
}
