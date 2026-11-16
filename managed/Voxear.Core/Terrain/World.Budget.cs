using Voxear.Interop;

namespace Voxear;

/// <summary>
/// What making worlds of blocks may spend, so a game runs on slow machines as well as fast ones, and looks its best
/// on fast ones. It starts as the project has it, in Project Settings ▸ World, or as this machine kept it. A game's
/// options change it while the game runs: worlds take it up at once, and nothing made is made again. SaveQuality
/// keeps it for this machine, so the game starts with it from then on.
/// <code>
/// World.Quality = WorldQuality.Ultra;
/// World.SaveQuality();
///
/// World.NearDetail = 1.25f;
/// World.PlantDetail = 2f;
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
	/// The furthest a world without end's own chunks reach, in metres, whatever its NearDetail. High stops at 192 m.
	/// </summary>
	public static float ViewDistance
	{
		get { return NativeWorldBudget.Get(NativeWorldBudget.ViewDistance); }
		set { NativeWorldBudget.Set(NativeWorldBudget.ViewDistance, value); }
	}

	/// <summary>The furthest its coarser chunks reach, in metres, whatever its FarDetail. High stops at 8 km.</summary>
	public static float FarDistance
	{
		get { return NativeWorldBudget.Get(NativeWorldBudget.FarDistance); }
		set { NativeWorldBudget.Set(NativeWorldBudget.FarDistance, value); }
	}

	/// <summary>
	/// How many times as far as the world's own View distance its own chunks reach: 1 at High, 1.5 at Ultra, from a
	/// quarter to four.
	/// </summary>
	public static float NearDetail
	{
		get { return NativeWorldBudget.Get(NativeWorldBudget.NearDetail); }
		set { NativeWorldBudget.Set(NativeWorldBudget.NearDetail, value); }
	}

	/// <summary>How many times as far as the world's own Far distance its coarser chunks reach: 2 at Ultra.</summary>
	public static float FarDetail
	{
		get { return NativeWorldBudget.Get(NativeWorldBudget.FarDetail); }
		set { NativeWorldBudget.Set(NativeWorldBudget.FarDetail, value); }
	}

	/// <summary>
	/// How many times as far as each Foliage says its plants are drawn, the woods they show as far off starting that
	/// much further out. Under 1, a world's structures are built into fewer of its coarser rings.
	/// </summary>
	public static float PlantDetail
	{
		get { return NativeWorldBudget.Get(NativeWorldBudget.PlantDetail); }
		set { NativeWorldBudget.Set(NativeWorldBudget.PlantDetail, value); }
	}

	/// <summary>
	/// The largest a coarser ring's blocks show where the ring begins, in pixels of a screen 1080 tall: the rings are
	/// pushed out, the world's own blocks drawn on past them, until they are no larger. 3 at Ultra, 4 at High and 6 at
	/// Medium; 0 draws the rings where the distances put them.
	/// </summary>
	public static float FarBlockPixels
	{
		get { return NativeWorldBudget.Get(NativeWorldBudget.FarBlockPixels); }
		set { NativeWorldBudget.Set(NativeWorldBudget.FarBlockPixels, value); }
	}

	/// <summary>Whether this machine keeps its own quality for the game, in place of the project's.</summary>
	public static bool QualitySaved => NativeWorldBudget.Get(NativeWorldBudget.KeptForMachine) == 1f;

	/// <summary>
	/// Keeps the quality in use for this machine, in Graphics.toml beside the game's saves: the game starts with it
	/// from then on.
	/// </summary>
	public static void SaveQuality()
	{
		NativeWorldBudget.Set(NativeWorldBudget.KeptForMachine, 1f);
	}

	/// <summary>Forgets the quality this machine kept, and goes back to the project's at once.</summary>
	public static void ResetQuality()
	{
		NativeWorldBudget.Set(NativeWorldBudget.KeptForMachine, 0f);
	}

	private static int Whole(int which)
	{
		var read = NativeWorldBudget.Get(which);
		return float.IsNaN(read) ? 0 : (int)read;
	}
}
