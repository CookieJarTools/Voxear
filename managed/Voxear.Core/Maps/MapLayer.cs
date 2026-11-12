using System;
using System.Globalization;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A layer of news on the maps: something a game tells its players of places, as how dangerous each is or who
/// holds it. A place's value in it runs from 0 to 1, and colours the ground there from one colour to another, each
/// colour's alpha being how strongly it shows. Make one once and keep it.
/// <code>
/// private static readonly MapLayer Threat = MapLayer.Create(Color.Clear, new Color(0.9f, 0.1f, 0.1f, 0.7f));
///
/// // The board says the dungeon to the north has grown.
/// MapKnowledge.Shown.Paint(Threat, dungeon.Position, 150f, 0.8f, today);
/// </code>
/// </summary>
public readonly struct MapLayer : IEquatable<MapLayer>
{
	private const int LayerText = 215;

	private static int made;

	private MapLayer(byte id)
	{
		Id = id;
	}

	internal byte Id { get; }

	/// <summary>
	/// A layer coloured from <paramref name="low"/>, where its value is 0, to <paramref name="high"/>, where it is 1.
	/// A game has up to 255.
	/// </summary>
	public static MapLayer Create(Color low, Color high)
	{
		made = Math.Min(made + 1, 255);
		var layer = new MapLayer((byte)made);
		layer.SetColors(low, high);
		return layer;
	}

	/// <summary>Changes the layer's colours; the maps draw their news again.</summary>
	public void SetColors(Color low, Color high)
	{
		var text = string.Create(CultureInfo.InvariantCulture, $"{Id} {low.R} {low.G} {low.B} {low.A} {high.R} {high.G} {high.B} {high.A}");
		NativeComponent.Tell(NativeComponent.Map, Obj.None, LayerText, text);
	}

	public static bool operator ==(MapLayer left, MapLayer right)
	{
		return left.Id == right.Id;
	}

	public static bool operator !=(MapLayer left, MapLayer right)
	{
		return left.Id != right.Id;
	}

	public bool Equals(MapLayer other)
	{
		return Id == other.Id;
	}

	public override bool Equals(object? other)
	{
		return other is MapLayer layer && Equals(layer);
	}

	public override int GetHashCode()
	{
		return Id;
	}
}
