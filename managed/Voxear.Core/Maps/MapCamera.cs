using System.Globalization;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// How one map is seen: which way it is turned, how far over the ground it is seen from, how far it is zoomed, and
/// where it looks. The player's pointer changes these on a map with PointerControl, and a script can at any time.
/// Get it from a Minimap's or a MapView's Camera, once that is on the screen.
/// <code>
/// var camera = Obj.Map.MapView!.Camera;
/// camera.LookAt(town.WorldPosition);
/// camera.Zoom = 8f;
/// camera.Turn = 45f;
///
/// // What the player is pointing at on the map.
/// if (camera.PlaceAt(Mouse.Position) is { } place)
/// {
///	 Map.AddMarker(place, MapIcon.Pin, "Go here");
/// }
/// </code>
/// <para>
/// What is set is done the next time the map is drawn, and what is read is the map as it was last drawn, so a value
/// set reads back a frame later.
/// </para>
/// </summary>
public sealed class MapCamera
{
	private const int ViewText = 300;

	private readonly MapElement view;

	internal MapCamera(MapElement view)
	{
		this.view = view;
	}

	/// <summary>How far it is turned from north, in degrees: at 0 north is at its top, at 90 east is.</summary>
	public float Turn
	{
		get { return Read(0); }
		set { Tell(Numbers("turn", value)); }
	}

	/// <summary>How far over the ground it is seen from, in degrees, from 20 to 90: 90 looks straight down.</summary>
	public float Tilt
	{
		get { return Read(1); }
		set { Tell(Numbers("tilt", value)); }
	}

	/// <summary>
	/// How many blocks a cell of the map is: any number within the view's MinZoom and MaxZoom, not only the powers
	/// of two. The map is drawn in cells of the nearest power of two and scaled to this.
	/// </summary>
	public float Zoom
	{
		get { return Read(2); }
		set { Tell(Numbers("zoom", value)); }
	}

	/// <summary>How far its middle is from what the maps follow, in metres east and north.</summary>
	public Vector2 Offset
	{
		get { return new Vector2(Read(3), Read(4)); }
		set { Tell(Numbers("offset", value.X, value.Y)); }
	}

	/// <summary>Moves its middle to a place in the scene. It stays that far from what the maps follow.</summary>
	public void LookAt(Vector3 place)
	{
		Tell(Numbers("look", place.X, place.Y, place.Z));
	}

	/// <summary>Puts it back as its settings have it: north at its top, over what the maps follow, at its Zoom and Tilt.</summary>
	public void Reset()
	{
		Tell("reset");
	}

	/// <summary>
	/// Where a place in the scene shows on the map, in the UI's pixels from the view's top left: null before the
	/// map was drawn, or for a place behind its camera.
	/// </summary>
	public Vector2? PointOf(Vector3 place)
	{
		var words = Words(Ask(Numbers("point", place.X, place.Y, place.Z)), 3);
		return words is null ? null : new Vector2(words[0], words[1]);
	}

	/// <summary>Whether a place in the scene is on the ground the map shows now.</summary>
	public bool Shows(Vector3 place)
	{
		var words = Words(Ask(Numbers("point", place.X, place.Y, place.Z)), 3);
		return words is not null && words[2] != 0f;
	}

	/// <summary>
	/// The place in the scene under a point of the view, in the UI's pixels from its top left: where the player
	/// points on the map. Null off the ground the map shows.
	/// </summary>
	public Vector3? PlaceAt(Vector2 point)
	{
		var words = Words(Ask(Numbers("place", point.X, point.Y)), 3);
		return words is null ? null : new Vector3(words[0], words[1], words[2]);
	}

	private float Read(int which)
	{
		var words = Words(Ask("get"), 5);
		return words is null ? 0f : words[which];
	}

	private static float[]? Words(string? answer, int count)
	{
		var words = (answer ?? string.Empty).Split(' ', System.StringSplitOptions.RemoveEmptyEntries);
		if (words.Length != count)
		{
			return null;
		}
		return System.Array.ConvertAll(words, word => float.Parse(word, CultureInfo.InvariantCulture));
	}

	private static string Numbers(string verb, params float[] numbers)
	{
		var words = System.Array.ConvertAll(numbers, number => number.ToString(CultureInfo.InvariantCulture));
		return $"{verb} {string.Join(' ', words)}";
	}

	private string? Ask(string asked)
	{
		if (!view.IsOnScreen)
		{
			return null;
		}
		var number = view.Id.ToString(CultureInfo.InvariantCulture);
		return NativeComponent.Ask(NativeComponent.Map, Obj.None, ViewText, $"{number} {asked}");
	}

	private void Tell(string told)
	{
		Ask(told);
	}
}
