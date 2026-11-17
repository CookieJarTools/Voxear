using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// Ways over the ground, for scripts that want to know one without a Nav Agent walking it: whether a place can be
/// reached, how far it is on foot, the nearest place to stand, somewhere to wander to. In a world of blocks the
/// places are made from its blocks as they stream in and change, so a block placed or dug counts from the next frame.
/// <code>
/// // Only chase what can be reached.
/// if (Navigation.IsReachable(Obj.Position, player.Position))
/// {
/// 	Obj.NavAgent.Destination = player.Position;
/// }
/// </code>
/// </summary>
public static class Navigation
{
	private const int FromValue = 3;
	private const int FindValue = 4;
	private const int FoundValue = 5;
	private const int CornerAtValue = 6;
	private const int CornerValue = 7;
	private const int NearestValue = 8;
	private const int NearestFoundValue = 9;
	private const int RandomValue = 11;
	private const int RandomFoundValue = 12;
	private const int AreaAtValue = 13;
	private const int AreaFoundValue = 14;

	/// <summary>
	/// The way from <paramref name="from"/> to <paramref name="to"/> for something <paramref name="height"/> metres
	/// tall, found now: NavPath.None where there is no ground to start from.
	/// </summary>
	public static NavPath FindPath(Vector3 from, Vector3 to, float height = 1.8f)
	{
		if (!NativeComponent.Set(NativeComponent.Navigation, Obj.None, FromValue, new Vector4(from, height)))
		{
			return NavPath.None;
		}

		NativeComponent.Set(NativeComponent.Navigation, Obj.None, FindValue, new Vector4(to, 0f));
		if (!NativeComponent.Get(NativeComponent.Navigation, Obj.None, FoundValue, out var found))
		{
			return NavPath.None;
		}

		var corners = new Vector3[(int)found.Y];
		for (var at = 0; at < corners.Length; at++)
		{
			NativeComponent.Set(NativeComponent.Navigation, Obj.None, CornerAtValue, new Vector4(at, 0f, 0f, 0f));
			NativeComponent.Get(NativeComponent.Navigation, Obj.None, CornerValue, out var corner);
			corners[at] = new Vector3(corner.X, corner.Y, corner.Z);
		}

		return new NavPath((NavPathStatus)(int)found.X, corners, found.Z);
	}

	/// <summary>Whether something <paramref name="height"/> metres tall can walk from one place to the other.</summary>
	public static bool IsReachable(Vector3 from, Vector3 to, float height = 1.8f)
	{
		return FindPath(from, to, height).Status == NavPathStatus.Complete;
	}

	/// <summary>
	/// The nearest place to stand to <paramref name="point"/>, on its floor, no more than <paramref name="reach"/>
	/// metres off; null where there is none.
	/// </summary>
	public static Vector3? Nearest(Vector3 point, float reach = 4f)
	{
		NativeComponent.Set(NativeComponent.Navigation, Obj.None, NearestValue, new Vector4(point, reach));
		var found = NativeComponent.Get(NativeComponent.Navigation, Obj.None, NearestFoundValue, out var place);
		return found && place.W != 0f ? new Vector3(place.X, place.Y, place.Z) : null;
	}

	/// <summary>
	/// The area of ground of the place to stand nearest <paramref name="point"/>, no more than
	/// <paramref name="reach"/> metres off: as its block or a Nav Area box marks it. Null where there is none.
	/// </summary>
	public static NavAreaType? AreaAt(Vector3 point, float reach = 4f)
	{
		NativeComponent.Set(NativeComponent.Navigation, Obj.None, AreaAtValue, new Vector4(point, reach));
		var found = NativeComponent.Get(NativeComponent.Navigation, Obj.None, AreaFoundValue, out var area);
		return found && area.Y != 0f ? (NavAreaType)(int)area.X : null;
	}

	/// <summary>
	/// A place that can be walked to from <paramref name="near"/>, no more than <paramref name="radius"/> metres
	/// away on foot and another each time: somewhere to wander to. Null where there is none.
	/// </summary>
	public static Vector3? RandomPlace(Vector3 near, float radius)
	{
		NativeComponent.Set(NativeComponent.Navigation, Obj.None, RandomValue, new Vector4(near, radius));
		var found = NativeComponent.Get(NativeComponent.Navigation, Obj.None, RandomFoundValue, out var place);
		return found && place.W != 0f ? new Vector3(place.X, place.Y, place.Z) : null;
	}
}
