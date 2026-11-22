using System.Text.Json.Nodes;
using Voxear.Interop;
using Voxear.Scripting;

namespace Voxear;

/// <summary>
/// An Obj's Route Stop: a place on a route, a harbour, an inn or a waystation, with a data asset of the game's own. It
/// joins the nearest route within reach, and a way Routes.Find gives lists the stops it passes. Get it with
/// Obj.RouteStop, or give an Obj one with Obj.AddRouteStop.
/// </summary>
public sealed class RouteStop : WholeComponent
{
	internal RouteStop(Obj obj)
		: base(obj, NativeWhole.Whole, NativeWhole.RouteStop)
	{
	}

	/// <summary>Its data asset; null for none.</summary>
	public Definition? Data
	{
		get => Settings()["data"] is JsonValue value && value.TryGetValue<string>(out var guid) ? Definitions.Load(typeof(Definition), guid) : null;
		set => Change("data", AreaJson.Key(value));
	}

	internal static RouteStop? Of(Obj obj)
	{
		var stop = new RouteStop(obj);
		return stop.IsValid ? stop : null;
	}

	public override string ToString()
	{
		return $"{Obj}'s Route Stop";
	}
}
