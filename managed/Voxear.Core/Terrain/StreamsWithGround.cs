using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Streams With Ground: the Obj comes and goes with the ground under it as a world of blocks without end
/// streams round the camera, while the game plays. Kept for later, it is made again exactly as it was when its ground
/// comes back; destroyed, it goes for good. Behaviours on it are told with OnGroundUnloading first, and those of one
/// kept for later with OnGroundLoaded when it is back. Get it with Obj.StreamsWithGround, or add it with
/// Obj.AddStreamsWithGround.
/// <code>
/// // A mob spawned round the player goes once the player has left its ground behind.
/// var wolf = Obj.Instantiate(WolfPrefab, at);
/// wolf.AddStreamsWithGround().WhenUnloaded = WhenUnloaded.Destroy;
/// </code>
/// </summary>
public sealed class StreamsWithGround : WholeComponent
{
	internal StreamsWithGround(Obj obj)
		: base(obj, NativeWhole.Whole, NativeWhole.StreamsWithGround)
	{
	}

	/// <summary>What happens to the Obj when its ground streams out.</summary>
	public WhenUnloaded WhenUnloaded
	{
		get => Choice("when_unloaded", WhenUnloaded.KeepForLater);
		set => Change("when_unloaded", PostJson.Of(value));
	}

	internal static StreamsWithGround? Of(Obj obj)
	{
		var streams = new StreamsWithGround(obj);
		return streams.IsValid ? streams : null;
	}

	public override string ToString()
	{
		return $"{Obj}'s Streams With Ground";
	}
}
