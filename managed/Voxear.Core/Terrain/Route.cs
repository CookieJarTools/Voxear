using System.Collections.Generic;
using System.Numerics;
using System.Text.Json.Nodes;
using Voxear.Interop;
using Voxear.Scripting;

namespace Voxear;

/// <summary>
/// An Obj's Route: a way through a world that travellers keep to, a line of points of a kind, as wide as it says, one
/// way or both. Every Road is a route of kind road. Routes of a kind that meet are joined, and Routes.Find gives the
/// way along them from one place to another. A sea route lays itself over deep water between its ends with
/// <see cref="LayOverSea"/>. Get it with Obj.Route, or give an Obj one with Obj.AddRoute.
/// <code>
/// var lane = Obj.AddRoute();
/// lane.Kind = RouteKind.Sea;
/// lane.Points = [harbour.Position, island.Position];
/// lane.LayOverSea();
/// </code>
/// </summary>
public sealed class Route : WholeComponent
{
	internal Route(Obj obj)
		: base(obj, NativeWhole.Whole, NativeWhole.Route)
	{
	}

	/// <summary>Its points, in the Obj's own space.</summary>
	public IReadOnlyList<Vector3> Points
	{
		get
		{
			var points = new List<Vector3>();
			if (Settings()["points"] is JsonArray read)
			{
				foreach (var node in read)
				{
					points.Add(MixJson.Three(node, Vector3.Zero));
				}
			}
			return points;
		}
		set
		{
			var points = new JsonArray();
			foreach (var point in value)
			{
				points.Add(PostJson.Of(point));
			}
			Change("points", points);
		}
	}

	/// <summary>What travels it.</summary>
	public RouteKind Kind
	{
		get => Choice("kind", RouteKind.Road);
		set => Change("kind", PostJson.Of(value));
	}

	/// <summary>The kind of the game's own a Custom route is.</summary>
	public CustomRouteKind? Custom
	{
		get => Settings()["custom"] is JsonValue value && value.TryGetValue<string>(out var guid) ? Definitions.Load(typeof(CustomRouteKind), guid) as CustomRouteKind : null;
		set => Change("custom", AreaJson.Key(value));
	}

	/// <summary>How wide it is, in metres.</summary>
	public float Width
	{
		get => Number("width", 6f);
		set => Change("width", value);
	}

	/// <summary>Whether it is travelled only from its first point towards its last.</summary>
	public bool OneWay
	{
		get => Switch("one_way", false);
		set => Change("one_way", value);
	}

	/// <summary>A sea route's least depth of water, in metres.</summary>
	public float Draught
	{
		get => Number("draught", 3f);
		set => Change("draught", value);
	}

	/// <summary>A sea route's least distance from land, in metres.</summary>
	public float Shore
	{
		get => Number("shore", 24f);
		set => Change("shore", value);
	}

	/// <summary>Whether the game's maps draw it.</summary>
	public bool OnMap
	{
		get => Switch("on_map", true);
		set => Change("on_map", value);
	}

	/// <summary>
	/// Lays it over the sea between its first and last points, over water deeper than its Draught and further than its
	/// Shore from land, searched over the world's generated ground however far off; whether the sea joins them.
	/// </summary>
	public bool LayOverSea()
	{
		var points = Points;
		if (points.Count < 2)
		{
			return false;
		}
		var placed = Native.GetWorldTransform(Obj.Id);
		var world = Matrix4x4.CreateScale(placed.Scale)
			* Matrix4x4.CreateFromQuaternion(placed.Rotation)
			* Matrix4x4.CreateTranslation(placed.Position);
		var from = Vector3.Transform(points[0], world);
		var to = Vector3.Transform(points[^1], world);
		if (Routes.LaySea(from, to, Draught, Shore) is not { } laid || !Matrix4x4.Invert(world, out var toLocal))
		{
			return false;
		}
		var local = new List<Vector3>();
		foreach (var point in laid)
		{
			local.Add(Vector3.Transform(point, toLocal));
		}
		Points = local;
		return true;
	}

	internal static Route? Of(Obj obj)
	{
		var route = new Route(obj);
		return route.IsValid ? route : null;
	}

	public override string ToString()
	{
		return $"{Obj}'s Route";
	}
}
