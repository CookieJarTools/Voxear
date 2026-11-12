using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Map: the scene's Block World shown as a small model of itself, as a minimap in a corner of the screen
/// and a map opened over the game by an input action. Add one to any Obj and both show, with no UI of your own. Get
/// it with Obj.Map, or give an Obj one with Obj.AddMap.
/// <code>
/// // A marker on an Obj, and one at a place.
/// Map.AddMarker(town, MapIcon.Town, "Oakrest");
/// var camp = Map.AddMarker(new Vector3(120f, 30f, -40f), MapIcon.Camp, "Old camp");
///
/// // Every map follows the player rather than the camera.
/// Map.Follow = player;
/// </code>
/// <para>
/// For a HUD of your own, leave the component out and put a Minimap or a MapView in your UI: they show the same
/// maps. What every map follows, its title and its markers are set here either way.
/// </para>
/// </summary>
public readonly struct Map : IEquatable<Map>, IObjComponent
{
	private const int MinimapValue = 3;
	private const int CornerValue = 4;
	private const int SizeValue = 5;
	private const int ZoomValue = 6;
	private const int TurnsValue = 7;
	private const int RoundValue = 8;
	private const int TiltValue = 9;
	private const int MapZoomValue = 10;
	private const int OpenValue = 11;
	private const int ShowsValue = 12;
	private const int FollowValue = 100;
	private const int PlaceClearValue = 105;
	private const int ActionRead = 1;
	private const int ActionWrite = 2;
	private const int TitleText = 100;

	public static readonly Map None = default;

	internal Map(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj it belongs to.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Map, or is gone.</summary>
	public bool IsValid => Obj.IsValid && Number(NativeComponent.Has) != 0f;

	/// <summary>Whether its minimap shows.</summary>
	public bool Minimap
	{
		get { return Number(MinimapValue) != 0f; }
		set { Write(MinimapValue, value ? 1f : 0f); }
	}

	/// <summary>The corner of the screen its minimap sits in.</summary>
	public MapCorner Corner
	{
		get { return (MapCorner)(int)Number(CornerValue); }
		set { Write(CornerValue, (int)value); }
	}

	/// <summary>How large its minimap is, in the UI's pixels.</summary>
	public float Size
	{
		get { return Number(SizeValue); }
		set { Write(SizeValue, value); }
	}

	/// <summary>How many blocks a cell of its minimap is: 1, 2, 4 and so on to 64. Another number takes the nearest.</summary>
	public int Zoom
	{
		get { return (int)Number(ZoomValue); }
		set { Write(ZoomValue, value); }
	}

	/// <summary>Whether its minimap turns with the view, what is ahead at its top, or keeps north at its top.</summary>
	public bool TurnsWithView
	{
		get { return Number(TurnsValue) != 0f; }
		set { Write(TurnsValue, value ? 1f : 0f); }
	}

	/// <summary>Whether its minimap's ground is cut round rather than square.</summary>
	public bool Round
	{
		get { return Number(RoundValue) != 0f; }
		set { Write(RoundValue, value ? 1f : 0f); }
	}

	/// <summary>How far over the ground its minimap is seen from, in degrees: 90 looks straight down.</summary>
	public float Tilt
	{
		get { return Number(TiltValue); }
		set { Write(TiltValue, value); }
	}

	/// <summary>How many blocks a cell of its map is when it opens.</summary>
	public int MapZoom
	{
		get { return (int)Number(MapZoomValue); }
		set { Write(MapZoomValue, value); }
	}

	/// <summary>
	/// The input action that opens and closes its map. While the map is open the mouse is free and only the input
	/// map this action is in stays on, so put it in a map of its own, or in the one your menus use.
	/// </summary>
	public InputActionRef OpenAction
	{
		get { return new InputActionRef(NativeComponent.Text(NativeComponent.Map, Obj, ActionRead) ?? string.Empty); }
		set { NativeComponent.Tell(NativeComponent.Map, Obj, ActionWrite, value.Id); }
	}

	/// <summary>
	/// What the maps show of the world: all of it, what was explored, or only what scripts reveal with
	/// MapKnowledge. Ground not known is drawn as blank paper.
	/// </summary>
	public MapShows Shows
	{
		get { return (MapShows)(int)Number(ShowsValue); }
		set { Write(ShowsValue, (int)value); }
	}

	/// <summary>Whether its map is open over the game.</summary>
	public bool IsOpen
	{
		get { return Number(OpenValue) != 0f; }
		set { Write(OpenValue, value ? 1f : 0f); }
	}

	/// <summary>
	/// What every map follows: its middle is where this Obj is, and a minimap that turns with the view turns with
	/// it. None, as it starts, follows the game's camera.
	/// </summary>
	public static Obj Follow
	{
		get { return NativeComponent.Get(NativeComponent.Map, Obj.None, FollowValue, out var value) ? NativeComponent.ObjOf(value) : Obj.None; }
		set { NativeComponent.Set(NativeComponent.Map, Obj.None, FollowValue, NativeComponent.ValueOf(value)); }
	}

	/// <summary>
	/// What an opened map's title says. Null, as it starts, names the country the map looks at by how warm and wet
	/// it is there.
	/// </summary>
	public static string? Title
	{
		set { NativeComponent.Tell(NativeComponent.Map, Obj.None, TitleText, value ?? string.Empty); }
	}

	/// <summary>Marks an Obj on every map, where it is: gives it a Map Marker, or sets the one it has.</summary>
	public static MapMarker AddMarker(Obj obj, MapIcon icon, string label = "", Color? color = null)
	{
		var marker = obj.AddMapMarker();
		marker.Icon = icon;
		marker.Label = label;
		if (color is { } picked)
		{
			marker.Color = picked;
		}
		return marker;
	}

	/// <summary>Marks a place on every map. Keep the pin to move it, restyle it or take it off.</summary>
	public static MapPin AddMarker(Vector3 position, MapIcon icon, string label = "", Color? color = null)
	{
		return new MapPin(position, icon, label, color ?? MapPin.DefaultColor);
	}

	/// <summary>Takes every pin off the maps. Markers on Objs stay.</summary>
	public static void ClearPins()
	{
		NativeComponent.Set(NativeComponent.Map, Obj.None, PlaceClearValue, Vector4.Zero);
	}

	/// <summary>Opens the map over the game.</summary>
	public void Open()
	{
		IsOpen = true;
	}

	/// <summary>Closes the map.</summary>
	public void Close()
	{
		IsOpen = false;
	}

	/// <summary>Takes the Map off its Obj, and its minimap and map off the screen.</summary>
	public void Remove()
	{
		NativeComponent.Set(NativeComponent.Map, Obj, NativeComponent.Remove, Vector4.Zero);
	}

	private float Number(int what)
	{
		return NativeComponent.Number(NativeComponent.Map, Obj, what, 0f);
	}

	private void Write(int what, float value)
	{
		NativeComponent.Set(NativeComponent.Map, Obj, what, new Vector4(value, 0f, 0f, 0f));
	}

	public static bool operator ==(Map left, Map right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(Map left, Map right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(Map other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is Map same && Equals(same);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Map";
	}
}
