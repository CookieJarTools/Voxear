using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Map Marker: marks the Obj on every map, where it is, with an icon in a colour and a label beside it on
/// a map opened large. Put one on the player's prefab and each player shows on the others' maps. Get it with
/// Obj.MapMarker, or give an Obj one with Obj.AddMapMarker or Map.AddMarker.
/// <code>
/// var marker = chest.AddMapMarker();
/// marker.Icon = MapIcon.Chest;
/// marker.Label = "Supplies";
/// marker.StaysAtEdge = true;
/// </code>
/// </summary>
public readonly struct MapMarker : IEquatable<MapMarker>, IObjComponent
{
	private const int IconValue = 3;
	private const int ColorValue = 4;
	private const int ShowsValue = 5;
	private const int EdgeValue = 6;
	private const int LabelRead = 1;
	private const int LabelWrite = 2;

	public static readonly MapMarker None = default;

	internal MapMarker(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj it belongs to.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Map Marker, or is gone.</summary>
	public bool IsValid => Obj.IsValid && NativeComponent.Number(NativeComponent.MapMarker, Obj, NativeComponent.Has, 0f) != 0f;

	/// <summary>What it is drawn as.</summary>
	public MapIcon Icon
	{
		get { return (MapIcon)(int)NativeComponent.Number(NativeComponent.MapMarker, Obj, IconValue, 0f); }
		set { Write(IconValue, new Vector4((int)value, 0f, 0f, 0f)); }
	}

	/// <summary>What is written beside it on a map opened large; empty for nothing.</summary>
	public string Label
	{
		get { return NativeComponent.Text(NativeComponent.MapMarker, Obj, LabelRead) ?? string.Empty; }
		set { NativeComponent.Tell(NativeComponent.MapMarker, Obj, LabelWrite, value); }
	}

	/// <summary>Its colour.</summary>
	public Color Color
	{
		get { return NativeComponent.Get(NativeComponent.MapMarker, Obj, ColorValue, out var value) ? new Color(value.X, value.Y, value.Z) : MapPin.DefaultColor; }
		set { Write(ColorValue, new Vector4(value.R, value.G, value.B, 1f)); }
	}

	/// <summary>Which maps it shows on.</summary>
	public MarkerShows Shows
	{
		get { return (MarkerShows)(int)NativeComponent.Number(NativeComponent.MapMarker, Obj, ShowsValue, 0f); }
		set { Write(ShowsValue, new Vector4((int)value, 0f, 0f, 0f)); }
	}

	/// <summary>Whether it stays at a map's edge, towards where it is, once it is past it.</summary>
	public bool StaysAtEdge
	{
		get { return NativeComponent.Number(NativeComponent.MapMarker, Obj, EdgeValue, 0f) != 0f; }
		set { Write(EdgeValue, new Vector4(value ? 1f : 0f, 0f, 0f, 0f)); }
	}

	/// <summary>Takes the Map Marker off its Obj.</summary>
	public void Remove()
	{
		Write(NativeComponent.Remove, Vector4.Zero);
	}

	private void Write(int what, Vector4 value)
	{
		NativeComponent.Set(NativeComponent.MapMarker, Obj, what, value);
	}

	public static bool operator ==(MapMarker left, MapMarker right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(MapMarker left, MapMarker right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(MapMarker other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is MapMarker same && Equals(same);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Map Marker";
	}
}
