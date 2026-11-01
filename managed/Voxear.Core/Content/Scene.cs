using System;
using System.Collections.Generic;
using Voxear.Interop;
using Voxear.Scripting;

namespace Voxear
{
	/// <summary>
	/// Single scenes, for games that want more say than levels give them: load one alongside the scenes loaded, in the
	/// background, load one again, unload one, or make one the active scene. The level stays the level, and its next
	/// change unloads the scenes it does not want, these too, unless that change is asked to keep them.
	/// <code>
	/// await Scene.LoadAsync("Scenes/Cellar.scene");
	/// Scene.SetActive("Scenes/Cellar.scene");
	/// Scene.Unload("Scenes/Cellar.scene");
	/// </code>
	/// <para>
	/// The latest load is Scene.Current, so a loading bar reads Scene.Progress without being handed it, and a held load
	/// is let in with Scene.Activate().
	/// </para>
	/// </summary>
	public static class Scene
	{
		/// <summary>The scene loading now with LoadAsync, or the last one that did.</summary>
		public static SceneLoad? Current { get; private set; }

		/// <summary>Whether a scene is coming in now, from LoadAsync.</summary>
		public static bool IsLoading => Current is { IsDone: false };

		/// <summary>How far the scene coming in has come, from 0 to 1; 1 when none is. It costs no more than a field.</summary>
		public static float Progress => Current is { IsDone: false } load ? load.Progress : 1f;

		/// <summary>Whether the scene coming in has everything read and waits to be let in with Activate.</summary>
		public static bool IsReady => Current is { IsDone: false, IsReady: true };

		/// <summary>The scene Objs made at runtime go into, and whose sky is used, by its path.</summary>
		public static string? Active => Level.ActiveScene;

		/// <summary>
		/// Every scene loaded now, by its path: the level's and those loaded alongside. Each read asks the engine and
		/// makes a new list, so read it once rather than every frame.
		/// </summary>
		public static IReadOnlyList<string> Loaded
		{
			get
			{
				var loaded = Native.IsConnected ? Native.LevelInfo(Native.LevelInfoLoaded, -1) : null;
				return (loaded ?? string.Empty).Split('\n', StringSplitOptions.RemoveEmptyEntries);
			}
		}

		/// <summary>Whether a scene is loaded, by its path. It reads Loaded, with what that costs.</summary>
		public static bool IsLoaded(string scene)
		{
			return ((IList<string>)Loaded).Contains(scene);
		}

		/// <summary>
		/// Loads a scene by project path or GUID in the background, alongside those loaded, as the options say. Await it
		/// to carry on once it is in; it throws SceneLoadException if it could not load. A scene loaded already stays as
		/// it is, unless the options ask for it to load again.
		/// </summary>
		public static SceneLoad LoadAsync(string scene, SceneLoadOptions? options = null)
		{
			options ??= new SceneLoadOptions();
			var load = new SceneLoad(Native.SceneLoadAsync(scene, options), scene, options);
			Current = load;
			return Loading.Track(load);
		}

		/// <summary>Loads a scene again, in the background, so everything in it starts afresh; or loads it, when it is not loaded.</summary>
		public static SceneLoad ReloadAsync(string scene)
		{
			return LoadAsync(scene, new SceneLoadOptions { Reload = true });
		}

		/// <summary>Lets the scene coming in arrive, when it was asked to hold. Does nothing otherwise.</summary>
		public static void Activate()
		{
			Current?.Activate();
		}

		/// <summary>Unloads a loaded scene at the end of the frame, its Objs with it, apart from persistent ones.</summary>
		public static void Unload(string scene)
		{
			Native.SceneUnload(scene);
		}

		/// <summary>Makes a loaded scene the active one at the end of the frame.</summary>
		public static void SetActive(string scene)
		{
			Native.SceneSetActive(scene);
		}
	}
}
