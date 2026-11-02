using System;
using Voxear.Interop;

namespace Voxear
{
	/// <summary>
	/// A scene of the project, as a field holds it: picked in the Inspector, or dragged there from the Project panel.
	/// Give it to Scene.LoadAsync to load the scene alongside those loaded, or to Scene.Unload. It names the scene by
	/// its ID, so renaming or moving the file keeps the field pointing at it, and nothing of the scene is read until it
	/// is loaded.
	/// <code>
	/// public SceneAsset Cellar;
	/// ...
	/// await Scene.LoadAsync(Cellar);
	/// </code>
	/// </summary>
	public readonly struct SceneAsset : IEquatable<SceneAsset>, IAsset<SceneAsset>
	{
		public static readonly SceneAsset None = default;

		private readonly string? reference;

		private SceneAsset(string reference)
		{
			this.reference = reference;
		}

		/// <summary>Whether it names a scene.</summary>
		public bool IsSet => !string.IsNullOrEmpty(reference);

		/// <summary>The scene's GUID or project path, as the scene or code gave it.</summary>
		public string Reference => reference ?? string.Empty;

		string? IAsset<SceneAsset>.AssetGuid => IsSet ? NativeSaves.Key(NativeSaves.KeyReference, 0, Reference) : null;

		/// <summary>
		/// The scene at a project path or GUID, as "Scenes/Night.scene". Nothing is read: the scene loads when it is
		/// asked for, so this costs nothing and never fails here.
		/// </summary>
		public static SceneAsset Load(string path)
		{
			return new SceneAsset(path);
		}

		public static bool operator ==(SceneAsset left, SceneAsset right)
		{
			return left.Equals(right);
		}

		public static bool operator !=(SceneAsset left, SceneAsset right)
		{
			return !left.Equals(right);
		}

		public bool Equals(SceneAsset other)
		{
			return Reference == other.Reference;
		}

		public override bool Equals(object? other)
		{
			return other is SceneAsset asset && Equals(asset);
		}

		public override int GetHashCode()
		{
			return Reference.GetHashCode();
		}

		public override string ToString()
		{
			return IsSet ? Reference : "no scene";
		}
	}
}
