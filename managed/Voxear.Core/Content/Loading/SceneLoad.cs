using Voxear.Interop;

namespace Voxear
{
	/// <summary>
	/// One scene coming in alongside those loaded, from Scene.LoadAsync: how far along it is and what went wrong. Await
	/// it to carry on once the scene is in; it throws SceneLoadException when the scene could not load. One asked to
	/// hold waits at IsReady until Activate is called.
	/// <code>
	/// var cellar = Scene.LoadAsync("Scenes/Cellar.scene", new() { Hold = true });
	/// await FadeOut();
	/// cellar.Activate();
	/// await cellar;
	/// </code>
	/// </summary>
	public sealed class SceneLoad : AsyncLoad
	{
		internal SceneLoad(int id, string scene, SceneLoadOptions options) : base(id)
		{
			Scene = scene;
			Options = options;
		}

		/// <summary>The scene coming in.</summary>
		public string Scene { get; }

		public SceneLoadOptions Options { get; }

		/// <summary>Everything is read in, and the scene waits to be let in with Activate. Progress stops at 0.9 meanwhile.</summary>
		public bool IsReady { get; private set; }

		/// <summary>Lets the scene in, when it was asked to hold. Does nothing otherwise.</summary>
		public void Activate()
		{
			if (!IsDone)
			{
				Native.AllowActivation(Id, true);
			}
		}

		public SceneLoadAwaiter GetAwaiter()
		{
			return new SceneLoadAwaiter(this);
		}

		internal override void Read(LoadProgress status)
		{
			IsReady = status.State == LoadProgress.Ready;
		}

		internal override string Failure()
		{
			return Native.LevelInfo(Native.LevelInfoError, Id) ?? base.Failure();
		}

		/// <summary>Tells Level.SceneLoaded's listeners, before anything awaiting it carries on.</summary>
		internal override void Finished()
		{
			if (Error == null)
			{
				Level.SceneCameIn(Scene);
			}
		}
	}
}
