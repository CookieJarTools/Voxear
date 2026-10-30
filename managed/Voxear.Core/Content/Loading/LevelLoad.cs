using Voxear.Interop;

namespace Voxear
{
	/// <summary>
	/// A level loading in the background, from Level.LoadAsync. Once loaded it changes over at the end of a frame,
	/// unless AllowActivation is false, when it waits at IsReady for a loading screen to choose its moment.
	/// </summary>
	public sealed class LevelLoad : AsyncLoad
	{
		private bool allowActivation = true;

		internal LevelLoad(int id) : base(id)
		{
		}

		/// <summary>False holds the change at IsReady; setting it back to true lets it happen at the end of the frame.</summary>
		public bool AllowActivation
		{
			get { return allowActivation; }
			set
			{
				allowActivation = value;
				Native.AllowActivation(Id, value);
			}
		}

		/// <summary>Loaded and waiting for AllowActivation. Progress stops at 0.9 meanwhile, as in Unity.</summary>
		public bool IsReady { get; private set; }

		public LoadAwaiter GetAwaiter()
		{
			return new LoadAwaiter(this);
		}

		internal override void Read(LoadProgress status)
		{
			IsReady = status.State == LoadProgress.Ready;
		}
	}
}
