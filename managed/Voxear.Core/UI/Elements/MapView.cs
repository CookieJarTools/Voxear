namespace Voxear;

/// <summary>
/// A map opened large in a UI of your own: the scene's Block World as a small model of itself, with the markers
/// and their labels, a title, a readout and a compass. With PointerControl, as it starts, a drag turns and tilts
/// it, a drag with the middle button moves it over the ground, and the wheel zooms it. It covers what it is in.
/// In markup it is <c>&lt;Map zoom="4" /&gt;</c>.
/// <code>
/// var map = screen.Root.Add(new MapView { Zoom = 8, Fill = 1.3f });
/// map.Visible = false;
/// </code>
/// <para>
/// Sheets style it by its classes: <c>map</c> for the whole of it, <c>map-picture</c> for the ground,
/// <c>map-overlay</c> and <c>map-label</c> for the markers and their labels, <c>map-title</c>, <c>map-readout</c>
/// and <c>map-compass</c>.
/// </para>
/// </summary>
public sealed class MapView : MapElement
{
	public MapView() : base(UiKind.Map, true)
	{
	}

	internal MapView(int id) : base(id, true)
	{
	}
}
