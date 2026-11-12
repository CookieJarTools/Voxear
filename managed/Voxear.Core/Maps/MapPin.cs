using System;
using System.Globalization;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A marker at a place on every map, made with Map.AddMarker: somewhere that is no Obj, as a town not loaded, a
/// note the player left, or the last place a thing was heard of. It stays until removed.
/// <code>
/// var rumour = Map.AddMarker(lastSeen, MapIcon.Skull, "Seen here", Color.Red);
/// rumour.Position = newerSighting;
/// rumour.Remove();
/// </code>
/// </summary>
public sealed class MapPin
{
	private const int AtValue = 101;
	private const int LookValue = 102;
	private const int ColorValue = 103;
	private const int RemoveValue = 104;
	private const int LabelText = 101;

	internal static readonly Color DefaultColor = new Color(0.95f, 0.75f, 0.2f);

	private static uint made;

	private readonly float id;
	private Vector3 position;
	private MapIcon icon;
	private string label;
	private Color color;
	private MarkerShows shows;
	private bool staysAtEdge;

	internal MapPin(Vector3 position, MapIcon icon, string label, Color color)
	{
		made++;
		id = BitConverter.UInt32BitsToSingle(made);
		this.position = position;
		this.icon = icon;
		this.label = label;
		this.color = color;
		SendPlace();
		SendLook();
		SendColor();
		SendLabel();
	}

	/// <summary>False once it was removed.</summary>
	public bool IsOnMap { get; private set; } = true;

	/// <summary>Where it is, in the scene.</summary>
	public Vector3 Position
	{
		get { return position; }
		set
		{
			position = value;
			SendPlace();
		}
	}

	/// <summary>What it is drawn as.</summary>
	public MapIcon Icon
	{
		get { return icon; }
		set
		{
			icon = value;
			SendLook();
		}
	}

	/// <summary>What is written beside it on a map opened large; empty for nothing.</summary>
	public string Label
	{
		get { return label; }
		set
		{
			label = value;
			SendLabel();
		}
	}

	/// <summary>Its colour.</summary>
	public Color Color
	{
		get { return color; }
		set
		{
			color = value;
			SendColor();
		}
	}

	/// <summary>Which maps it shows on.</summary>
	public MarkerShows Shows
	{
		get { return shows; }
		set
		{
			shows = value;
			SendLook();
		}
	}

	/// <summary>Whether it stays at a map's edge, towards where it is, once it is past it.</summary>
	public bool StaysAtEdge
	{
		get { return staysAtEdge; }
		set
		{
			staysAtEdge = value;
			SendLook();
		}
	}

	/// <summary>Takes it off the maps for good.</summary>
	public void Remove()
	{
		if (IsOnMap)
		{
			Send(RemoveValue, new Vector4(id, 0f, 0f, 0f));
			IsOnMap = false;
		}
	}

	private void SendPlace()
	{
		Send(AtValue, new Vector4(id, position.X, position.Y, position.Z));
	}

	private void SendLook()
	{
		Send(LookValue, new Vector4(id, (int)icon, (int)shows, staysAtEdge ? 1f : 0f));
	}

	private void SendColor()
	{
		Send(ColorValue, new Vector4(id, color.R, color.G, color.B));
	}

	private void SendLabel()
	{
		if (IsOnMap)
		{
			var number = BitConverter.SingleToUInt32Bits(id).ToString(CultureInfo.InvariantCulture);
			NativeComponent.Tell(NativeComponent.Map, Obj.None, LabelText, $"{number}\n{label}");
		}
	}

	private void Send(int what, Vector4 value)
	{
		if (IsOnMap)
		{
			NativeComponent.Set(NativeComponent.Map, Obj.None, what, value);
		}
	}
}
