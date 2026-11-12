using System.Globalization;

namespace Voxear;

/// <summary>
/// A minimap in a UI of your own: the ground round what the maps follow (see Map.Follow), drawn as a small model
/// of the scene's Block World, with the markers on it. Give it a size in its style. In markup it is
/// <c>&lt;Minimap zoom="2" turns="true" /&gt;</c>.
/// <code>
/// var minimap = hud.Add(new Minimap { Zoom = 4, Round = true });
/// minimap.AddClass("corner");
/// </code>
/// <para>
/// Sheets style it by its classes: <c>minimap</c> for the whole of it, <c>map-picture</c> for the ground and
/// <c>map-overlay</c> for what lies over it.
/// </para>
/// </summary>
public class Minimap : UiElement
{
	private const int StringMapView = 4;

	private int zoom = 2;
	private bool turnsWithView = true;
	private bool round;
	private float tilt = 58f;

	public Minimap() : base(UiKind.Minimap, string.Empty)
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
