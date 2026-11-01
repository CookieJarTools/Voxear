namespace Voxear
{
	/// <summary>
	/// How a level change is made, as LayerCake's SceneLoadOptions are: straight through, or held at the last
	/// moment for a loading screen, taking at least some time, and keeping or letting go of what was there.
	/// </summary>
	public sealed class LevelLoadOptions
	{
		/// <summary>Straight through: the new level comes in as soon as it is read, and what was there goes.</summary>
		public static LevelLoadOptions Straight => new LevelLoadOptions();

		/// <summary>Held once everything is read in, until LevelLoad.Activate is called: time for a loading screen to finish.</summary>
		public static LevelLoadOptions Held => new LevelLoadOptions { Hold = true };

		/// <summary>Whether to wait to be let in once everything is read. Off by default.</summary>
		public bool Hold { get; set; }

		/// <summary>Seconds a change takes at the least, however quickly the level loads: long enough for a loading screen to be seen.</summary>
		public float LeastTime { get; set; }

		/// <summary>Whether the scenes loaded before, and not wanted by the new level, are unloaded. On by default.</summary>
		public bool UnloadRest { get; set; } = true;

		/// <summary>Whether the new level's active scene becomes the active one. On by default.</summary>
		public bool SetActive { get; set; } = true;

		/// <summary>Whether what nothing uses any more is let go once the change is over. On by default.</summary>
		public bool TidyUp { get; set; } = true;

		/// <summary>
		/// Whether the scenes the new level shares with the old one load again, as if each were marked Reload in the
		/// level, so everything in them starts afresh. Off by default, when shared scenes stay as they are.
		/// </summary>
		public bool ReloadShared { get; set; }
	}
}
