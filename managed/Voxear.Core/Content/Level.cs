using Voxear.Interop;
using Voxear.Scripting;

namespace Voxear
{
	/// <summary>
	/// Loads levels. A level is a list of scenes: scenes the current and next level share stay loaded,
	/// the rest unload, and Objs marked persistent survive every change.
	/// </summary>
	public static class Level
	{
		/// <summary>
		/// Loads a level by project path or GUID, for example "Levels/Night.level". The change happens
		/// after the current frame, so everything in this frame finishes first.
		/// </summary>
		public static void Load(string level)
		{
			Native.LoadLevel(level);
		}

		/// <summary>
		/// Loads a level in the background while the game carries on. It changes over at the end of the frame it
		/// finishes in; pass activate false to hold it at IsReady until AllowActivation is set, for a loading screen.
		/// Await it to continue once the new level is in.
		/// </summary>
		public static LevelLoad LoadAsync(string level, bool activate = true)
		{
			var load = new LevelLoad(Native.LoadAsync(true, level));
			if (!activate)
			{
				load.AllowActivation = false;
			}
			return Loading.Track(load);
		}
	}
}
