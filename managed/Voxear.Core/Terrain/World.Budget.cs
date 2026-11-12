using Voxear.Interop;

namespace Voxear;

/// <summary>
/// What making worlds of blocks may spend, so a game runs on slow machines as well as fast ones. It starts as the
/// project has it, in Project Settings ▸ World, and a game's options can change it while the game runs: worlds take
/// it up at once, and nothing made is made again.
/// <code>
/// World.Quality = WorldQuality.Low;
/// World.ViewDistance = 96f;
/// </code>
/// </summary>
public static partial class World
{
	/// <summary>
	/// The preset. Setting one sets every number under it; setting a number makes it Custom. High without the engine.
	/// </summary>
	public static WorldQuality Quality
	{
		get
		{
			var read = NativeWorldBudget.Get(NativeWorldBudget.Quality);
			return float.IsNaN(read) ? WorldQuality.High : (WorldQuality)(int)read;
		}
		set { NativeWorldBudget.Set(NativeWorldBudget.Quality, (int)value); }
	}

	/// <summary>How many threads make terrain at once, one at least. Low uses two.</summary>
	public static int Threads
	{
		get { return Whole(NativeWorldBudget.Threads); }
		set { NativeWorldBudget.Set(NativeWorldBudget.Threads, value); }
	}

	/// <summary>How many chunk columns a world without end makes at once, the nearest first. Low makes two.</summary>
	public static int ColumnsAtOnce
	{
		get { return Whole(NativeWorldBudget.Columns); }
		set { NativeWorldBudget.Set(NativeWorldBudget.Columns, value); }
	}

	/// <summary>
	/// How many megabytes of chunks go to the GPU a frame. A world with more to show comes over several frames, shown as
	/// it was until then.
	/// </summary>
	public static int UploadMegabytes
	{
		get { return Whole(NativeWorldBudget.Upload); }
		set { NativeWorldBudget.Set(NativeWorldBudget.Upload, value); }
	}

	/// <summary>
	/// How far a world without end's own chunks reach, in metres: no further than the world's own View distance.
	/// </summary>
	public static float ViewDistance
	{
		get { return NativeWorldBudget.Get(NativeWorldBudget.ViewDistance); }
		set { NativeWorldBudget.Set(NativeWorldBudget.ViewDistance, value); }
	}

	/// <summary>How far its coarser chunks reach, in metres: no further than the world's own Far distance.</summary>
	public static float FarDistance
	{
		get { return NativeWorldBudget.Get(NativeWorldBudget.FarDistance); }
		set { NativeWorldBudget.Set(NativeWorldBudget.FarDistance, value); }
	}

	private static int Whole(int which)
	{
		var read = NativeWorldBudget.Get(which);
		return float.IsNaN(read) ? 0 : (int)read;
	}
}
