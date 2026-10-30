using Voxear.Interop;

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
	}
}
