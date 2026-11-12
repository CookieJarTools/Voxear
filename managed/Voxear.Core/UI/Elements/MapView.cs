using System.Globalization;

namespace Voxear;

/// <summary>
/// A map opened large in a UI of your own: the scene's Block World as a small model of itself, with the markers
/// and their labels, a title, a readout and a compass. A drag turns and tilts it, a drag with the middle button
/// moves it over the ground, and the wheel zooms it. It covers what it is in. In markup it is
/// <c>&lt;Map zoom="4" /&gt;</c>.
/// <code>
/// var map = screen.Add(new MapView { Zoom = 8 });
/// map.Visible = false;
/// </code>
/// <para>
/// Sheets style it by its classes: <c>map</c> for the whole of it, <c>map-picture</c> for the ground,
/// <c>map-overlay</c> and <c>map-label</c> for the markers and their labels, <c>map-title</c>, <c>map-readout</c>
/// and <c>map-compass</c>.
/// </para>
/// </summary>
public class MapView : UiElement
{
	private const int StringMapView = 4;

	private int zoom = 4;
	private bool turnsWithView = false;
	private bool round;
	private float tilt = 42f;

	public MapView() : base(UiKind.Map, string.Empty)
	{
	}

	/// <summary>How many blocks a cell of it is: 1, 2, 4 and so on to 64. Another number takes the nearest.</summary>
	public int Zoom
	{
		get { return zoom; }
		set
		{
			zoom = value;
			Send();
		}
	}

	/// <summary>Whether it turns with what the maps follow, what is ahead at its top, or keeps north at its top.</summary>
	public bool TurnsWithView
	{
		get { return turnsWithView; }
		set
		{
			turnsWithView = value;
			Send();
		}
	}

	/// <summary>Whether its ground is cut round rather than square.</summary>
	public bool Round
	{
		get { return round; }
		set
		{
			round = value;
			Send();
		}
	}

	/// <summary>How far over the ground it is seen from, in degrees: 90 looks straight down.</summary>
	public float Tilt
	{
		get { return tilt; }
		set
		{
			tilt = value;
			Send();
		}
	}

	private protected override void OnShown()
	{
		Send();
	}

	private void Send()
	{
		var turns = turnsWithView ? 1 : 0;
		var cut = round ? 1 : 0;
		SendString(StringMapView, string.Create(CultureInfo.InvariantCulture, $"{zoom} {turns} {cut} {tilt}"));
	}
}
