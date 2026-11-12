using System;
using System.Globalization;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// What a Minimap and a MapView share: how the ground is shown, which parts show with it, and the Camera it is
/// seen through. Every setting can be changed while it shows.
/// </summary>
public abstract class MapElement : UiElement
{
	private const int StringMapView = 4;
	private const int ViewText = 300;

	private readonly bool made;
	private int zoom;
	private bool turnsWithView;
	private bool round;
	private float tilt;
	private float fill;
	private bool pointerControl;
	private int minZoom = 1;
	private int maxZoom = 64;
	private float shadows = 0.4f;
	private float edgeLines = 0.6f;
	private float thickness;
	private bool showArrow = true;
	private bool arrowStaysAtEdge = true;
	private bool showLabels;
	private bool showTitle = true;
	private bool showReadout = true;
	private bool showCompass = true;
	private bool showZoom = true;
	private float referenceHeight = 1080f;
	private MapCamera? camera;

	private protected MapElement(UiKind kind, bool large) : base(kind, string.Empty)
	{
		made = true;
		SetDefaults(large);
	}

	/// <summary>One the engine made, as a Map's own minimap and map are: its settings are read from it.</summary>
	private protected MapElement(int id, bool large) : base(id)
	{
		SetDefaults(large);
		var text = NativeComponent.Ask(NativeComponent.Map, Obj.None, ViewText, $"{id.ToString(CultureInfo.InvariantCulture)} settings");
		foreach (var pair in (text ?? string.Empty).Split(' ', StringSplitOptions.RemoveEmptyEntries))
		{
			var parts = pair.Split('=');
			Take(parts[0], parts.Length > 1 ? parts[1] : string.Empty);
		}
	}

	/// <summary>How it is seen, to read or to steer: its turn, tilt, zoom and where it looks.</summary>
	public MapCamera Camera => camera ??= new MapCamera(this);

	internal bool IsOnScreen => IsShown;

	/// <summary>How many blocks a cell of it is to begin with: 1, 2, 4 and so on to 64. Another number takes the nearest. Its Camera zooms on from here.</summary>
	public int Zoom
	{
		get { return zoom; }
		set
		{
			zoom = value;
			Send("zoom", value.ToString(CultureInfo.InvariantCulture));
		}
	}

	/// <summary>Whether it turns with what the maps follow, what is ahead at its top, or keeps north at its top.</summary>
	public bool TurnsWithView
	{
		get { return turnsWithView; }
		set
		{
			turnsWithView = value;
			Send("turns", value ? "true" : "false");
		}
	}

	/// <summary>Whether its ground is cut round rather than square.</summary>
	public bool Round
	{
		get { return round; }
		set
		{
			round = value;
			Send("round", value ? "true" : "false");
		}
	}

	/// <summary>How far over the ground it is seen from to begin with, in degrees: 90 looks straight down.</summary>
	public float Tilt
	{
		get { return tilt; }
		set
		{
			tilt = value;
			Send("tilt", value.ToString(CultureInfo.InvariantCulture));
		}
	}

	/// <summary>How wide its ground is drawn, as a share of the view's height: at 1 it is as wide as the view is high.</summary>
	public float Fill
	{
		get { return fill; }
		set
		{
			fill = value;
			Send("fill", value.ToString(CultureInfo.InvariantCulture));
		}
	}

	/// <summary>Whether the pointer turns, moves and zooms it: a drag, a drag with the middle button, and the wheel.</summary>
	public bool PointerControl
	{
		get { return pointerControl; }
		set
		{
			pointerControl = value;
			Send("controls", value ? "true" : "false");
		}
	}

	/// <summary>The fewest blocks a cell may be as it is zoomed in: 1 to 64.</summary>
	public int MinZoom
	{
		get { return minZoom; }
		set
		{
			minZoom = value;
			Send("min-zoom", value.ToString(CultureInfo.InvariantCulture));
		}
	}

	/// <summary>The most blocks a cell may be as it is zoomed out: 1 to 64.</summary>
	public int MaxZoom
	{
		get { return maxZoom; }
		set
		{
			maxZoom = value;
			Send("max-zoom", value.ToString(CultureInfo.InvariantCulture));
		}
	}

	/// <summary>How dark its shadows are, from 0 for none to 1.</summary>
	public float Shadows
	{
		get { return shadows; }
		set
		{
			shadows = value;
			Send("shadows", value.ToString(CultureInfo.InvariantCulture));
		}
	}

	/// <summary>How dark the lines along its blocks' edges are, from 0 for none to 1.</summary>
	public float EdgeLines
	{
		get { return edgeLines; }
		set
		{
			edgeLines = value;
			Send("edges", value.ToString(CultureInfo.InvariantCulture));
		}
	}

	/// <summary>How thick its ground is under its lowest place, in cells.</summary>
	public float Thickness
	{
		get { return thickness; }
		set
		{
			thickness = value;
			Send("thick", value.ToString(CultureInfo.InvariantCulture));
		}
	}

	/// <summary>Whether what the maps follow is marked with an arrow.</summary>
	public bool ShowArrow
	{
		get { return showArrow; }
		set
		{
			showArrow = value;
			Send("arrow", value ? "true" : "false");
		}
	}

	/// <summary>Whether that arrow stays at the ground's edge, towards what it marks, once the map is moved off it.</summary>
	public bool ArrowStaysAtEdge
	{
		get { return arrowStaysAtEdge; }
		set
		{
			arrowStaysAtEdge = value;
			Send("arrow-at-edge", value ? "true" : "false");
		}
	}

	/// <summary>Whether markers' labels show.</summary>
	public bool ShowLabels
	{
		get { return showLabels; }
		set
		{
			showLabels = value;
			Send("labels", value ? "true" : "false");
		}
	}

	/// <summary>Whether a map's title shows.</summary>
	public bool ShowTitle
	{
		get { return showTitle; }
		set
		{
			showTitle = value;
			Send("title", value ? "true" : "false");
		}
	}

	/// <summary>Whether a map's readout shows: where it looks, how warm and wet it is there, and its zoom.</summary>
	public bool ShowReadout
	{
		get { return showReadout; }
		set
		{
			showReadout = value;
			Send("readout", value ? "true" : "false");
		}
	}

	/// <summary>Whether a map's compass shows.</summary>
	public bool ShowCompass
	{
		get { return showCompass; }
		set
		{
			showCompass = value;
			Send("compass", value ? "true" : "false");
		}
	}

	/// <summary>Whether its zoom shows: a map's buttons, a minimap's tag.</summary>
	public bool ShowZoom
	{
		get { return showZoom; }
		set
		{
			showZoom = value;
			Send("zoom-control", value ? "true" : "false");
		}
	}

	/// <summary>
	/// How high the screen its parts' sizes are for is, in pixels: they are scaled by the screen's height over
	/// this, so the map takes up as much of a small window as of a large screen. 0 leaves them as they are.
	/// </summary>
	public float ReferenceHeight
	{
		get { return referenceHeight; }
		set
		{
			referenceHeight = value;
			Send("reference", value.ToString(CultureInfo.InvariantCulture));
		}
	}

	private protected override void OnShown()
	{
		if (!made)
		{
			return;
		}
		var settings = string.Create(CultureInfo.InvariantCulture, $"zoom={zoom} turns={turnsWithView} round={round} tilt={tilt} fill={fill} controls={pointerControl} min-zoom={minZoom} max-zoom={maxZoom} shadows={shadows} edges={edgeLines} thick={thickness} arrow={showArrow} arrow-at-edge={arrowStaysAtEdge} labels={showLabels} title={showTitle} readout={showReadout} compass={showCompass} zoom-control={showZoom} reference={referenceHeight}");
		SendString(StringMapView, settings.ToLowerInvariant());
	}

	private void SetDefaults(bool large)
	{
		zoom = large ? 4 : 2;
		turnsWithView = !large;
		tilt = large ? 42f : 58f;
		fill = large ? 1.15f : 1.7f;
		pointerControl = large;
		thickness = large ? 4f : 2f;
		showLabels = large;
	}

	private void Send(string name, string value)
	{
		SendString(StringMapView, $"{name}={value}");
	}

	private void Take(string name, string value)
	{
		var on = value == "true";
		float.TryParse(value, NumberStyles.Float, CultureInfo.InvariantCulture, out var number);
		switch (name)
		{
			case "zoom": zoom = (int)number; break;
			case "turns": turnsWithView = on; break;
			case "round": round = on; break;
			case "tilt": tilt = number; break;
			case "fill": fill = number; break;
			case "controls": pointerControl = on; break;
			case "min-zoom": minZoom = (int)number; break;
			case "max-zoom": maxZoom = (int)number; break;
			case "shadows": shadows = number; break;
			case "edges": edgeLines = number; break;
			case "thick": thickness = number; break;
			case "arrow": showArrow = on; break;
			case "arrow-at-edge": arrowStaysAtEdge = on; break;
			case "labels": showLabels = on; break;
			case "title": showTitle = on; break;
			case "readout": showReadout = on; break;
			case "compass": showCompass = on; break;
			case "zoom-control": showZoom = on; break;
			case "reference": referenceHeight = number; break;
		}
	}
}
