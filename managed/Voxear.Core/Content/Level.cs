using System;
using System.Runtime.Loader;
using Voxear.Interop;
using Voxear.Scripting;

namespace Voxear
{
	/// <summary>
	/// Loads levels, as CookieJarTools' LayerCake does. A level is a list of scenes, and may stand on another whose
	/// scenes are always there: scenes the current and next level share stay loaded, the rest unload, and Objs marked
	/// persistent survive every change.
	/// <code>
	/// await Level.LoadAsync("Levels/Night.level");
	/// </code>
	/// <para>
	/// The change going on is Level.Current, and Level.Progress, Level.IsReady and Level.Activate() read and drive it, so
	/// a loading screen needs no handle: start the change anywhere, and bind a bar to Level.Progress. They cost no more
	/// than reading a field. The events are static too, so anything can hear about a change without being handed
	/// anything: pause a simulation on SwitchStarted, move a bar on SwitchProgressed, begin playing on SwitchFinished.
	/// Only one change happens at a time; asking for another while one is going gives back the one already going, and
	/// says so in the log. For single scenes, see Scene.
	/// </para>
	/// </summary>
	public static class Level
	{
		/// <summary>A change has begun: the level being left and the one coming.</summary>
		public static event Action<LevelSwitch>? SwitchStarted;

		/// <summary>Raised every frame while the new level is read in.</summary>
		public static event Action<LevelLoad>? SwitchProgressed;

		/// <summary>Everything is read in and waiting to be let in; only for a change asked to hold.</summary>
		public static event Action<LevelLoad>? SwitchReady;

		/// <summary>A scene is in, by its path: one of a new level's, or one loaded alongside with Scene.LoadAsync.</summary>
		public static event Action<string>? SceneLoaded;

		/// <summary>The change is over and the new level is there.</summary>
		public static event Action<LevelSwitch>? SwitchFinished;

		/// <summary>A scene that had to load did not, or the level could not be read; the change stopped where it was.</summary>
		public static event Action<LevelLoad>? SwitchFailed;

		/// <summary>
		/// Lets go of listeners the scripts added before they were reloaded, which would otherwise run the old code and
		/// keep it in memory. Listeners subscribe again in OnEnable, which the reload runs.
		/// </summary>
		internal static void Forget(AssemblyLoadContext old)
		{
			SwitchStarted = GameCode.Without(SwitchStarted, old);
			SwitchProgressed = GameCode.Without(SwitchProgressed, old);
			SwitchReady = GameCode.Without(SwitchReady, old);
			SceneLoaded = GameCode.Without(SceneLoaded, old);
			SwitchFinished = GameCode.Without(SwitchFinished, old);
			SwitchFailed = GameCode.Without(SwitchFailed, old);
		}

		/// <summary>The level loaded now, by its path, including one started from the editor; null before the first.</summary>
		public static string? Active => Native.LevelInfo(Native.LevelInfoCurrent, -1);

		/// <summary>The scene new Objs go into and whose sky and post-processing are used, by its path.</summary>
		public static string? ActiveScene => Native.LevelInfo(Native.LevelInfoActiveScene, -1);

		/// <summary>The change going on now, or the last one there was.</summary>
		public static LevelLoad? Current { get; private set; }

		public static bool IsSwitching => Current != null && !Current.IsDone;

		/// <summary>The level coming in, or null when no change is going on.</summary>
		public static string? Next => Current is { IsDone: false } load ? load.Level : null;

		/// <summary>How far the change going on has come, from 0 to 1; 1 when none is. For a loading bar, read each frame.</summary>
		public static float Progress => Current is { IsDone: false } load ? load.Progress : 1f;

		/// <summary>Whether a held change has everything read in and waits to be let in with Activate.</summary>
		public static bool IsReady => Current is { IsDone: false, IsReady: true };

		/// <summary>Why the last change failed, or null when it did not.</summary>
		public static string? Error => Current?.Error;

		/// <summary>Lets a held change in, at the end of the frame. Does nothing when no change is held.</summary>
		public static void Activate()
		{
			if (Current is { IsDone: false } load)
			{
				load.Activate();
			}
		}

		/// <summary>
		/// Loads a level by project path or GUID, for example "Levels/Night.level". The change happens after the current
		/// frame, so everything in this frame finishes first. The events are not raised for it; use LoadAsync to hear.
		/// </summary>
		public static void Load(string level)
		{
			Native.LoadLevel(level);
		}

		/// <summary>
		/// Loads a level in the background while the game carries on, as the options say: straight through when left
		/// out. Await it to carry on once the new level is in; it throws LevelLoadException if the change failed.
		/// </summary>
		public static LevelLoad LoadAsync(string level, LevelLoadOptions? options = null)
		{
			if (Current != null && !Current.IsDone)
			{
				Log.Warning($"Level.LoadAsync(\"{level}\") while {Current.Level} is still coming in; that change goes on instead.");
				return Current;
			}

			options ??= LevelLoadOptions.Straight;
			var load = new LevelLoad(Native.LevelLoadAsync(level, options), level, options, Active);
			Current = load;
			Raise(SwitchStarted, new LevelSwitch(load.Leaving, load));
			if (load.IsDone)
			{
				Raise(SwitchFailed, load);
				return load;
			}

			return Loading.Track(load);
		}

		/// <summary>Loads a level in the background; pass activate false to hold it at IsReady until it is let in.</summary>
		public static LevelLoad LoadAsync(string level, bool activate)
		{
			return LoadAsync(level, activate ? LevelLoadOptions.Straight : LevelLoadOptions.Held);
		}

		/// <summary>Changes to the level a field holds, at the end of the frame, as Load(string) does.</summary>
		public static void Load(LevelAsset level)
		{
			Load(level.Reference);
		}

		/// <summary>
		/// Loads the level a field holds in the background, as the options say, as LoadAsync(string) does. A field with
		/// no level picked gives a change that failed, which the Console says.
		/// </summary>
		public static LevelLoad LoadAsync(LevelAsset level, LevelLoadOptions? options = null)
		{
			return LoadAsync(level.Reference, options);
		}

		internal static void Progressed(LevelLoad load)
		{
			Raise(SwitchProgressed, load);
		}

		internal static void WentReady(LevelLoad load)
		{
			Raise(SwitchReady, load);
		}

		internal static void Finished(LevelLoad load)
		{
			foreach (var scene in load.Scenes)
			{
				Raise(SceneLoaded, scene);
			}

			Raise(SwitchFinished, new LevelSwitch(load.Leaving, load));
		}

		internal static void Failed(LevelLoad load)
		{
			Raise(SwitchFailed, load);
		}

		internal static void SceneCameIn(string scene)
		{
			Raise(SceneLoaded, scene);
		}

		/// <summary>A listener that throws is logged, so the rest still hear and the change goes on.</summary>
		private static void Raise<T>(Action<T>? listeners, T value)
		{
			if (listeners == null)
			{
				return;
			}

			foreach (var listener in listeners.GetInvocationList())
			{
				Hear((Action<T>)listener, value);
			}
		}

		private static void Hear<T>(Action<T> listener, T value)
		{
			try
			{
				listener(value);
			}
			catch (Exception exception)
			{
				Log.Exception(exception);
			}
		}
	}
}
