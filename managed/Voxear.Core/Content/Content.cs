using Voxear.Interop;

namespace Voxear
{
	/// <summary>
	/// Control over what stays loaded. Most games never need it: every level change frees what the old level used and
	/// the new one does not, as Unity's UnloadUnusedAssets would. Prefabs also have Keep and Release, and levels and
	/// prefabs can load ahead of time with LoadAsync.
	/// </summary>
	public static class Content
	{
		/// <summary>
		/// What is loaded now. It reads the engine's counts, so it costs a call; check it when needed rather than every
		/// frame.
		/// </summary>
		public static ContentUsage Usage => Native.ContentUsage();

		/// <summary>
		/// Frees, at the end of the frame, whatever no live Obj and no kept or recently used prefab needs: meshes,
		/// materials, textures and prefab data. Anything freed loads again by itself if it is used later. Use it after
		/// destroying many Objs within a level, such as after a boss fight.
		/// </summary>
		public static void UnloadUnused()
		{
			Native.ContentUnloadUnused();
		}
	}
}
