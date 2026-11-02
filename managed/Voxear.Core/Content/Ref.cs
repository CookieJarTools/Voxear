using System;

namespace Voxear;

/// <summary>
/// A reference to an asset that stays unloaded until it is asked for, like an Addressables AssetReference. A Prefab
/// field is loaded with its Obj, before Awake. A Ref&lt;Prefab&gt; field only names the prefab, and Load brings it
/// in when the game needs it, so rarely used assets cost nothing until then. Scenes write both the same way: a
/// project path or GUID.
/// </summary>
public readonly struct Ref<T> : IEquatable<Ref<T>> where T : struct, IAsset<T>
{
	private readonly string? path;

	public Ref(string path)
	{
		this.path = path;
	}

	/// <summary>The project path or GUID, as the scene or prefab wrote it.</summary>
	public string Path => path ?? string.Empty;

	public bool IsSet => !string.IsNullOrEmpty(path);

	/// <summary>Loads the asset, or returns the already loaded one. Logs an error when nothing is set.</summary>
	public T Load()
	{
		if (!IsSet)
		{
			Log.Error($"{this} names no asset to load.");
			return default;
		}
		return T.Load(Path);
	}

	public static bool operator ==(Ref<T> left, Ref<T> right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(Ref<T> left, Ref<T> right)
	{
		return !left.Equals(right);
	}

	public bool Equals(Ref<T> other)
	{
		return Path == other.Path;
	}

	public override bool Equals(object? other)
	{
		return other is Ref<T> reference && Equals(reference);
	}

	public override int GetHashCode()
	{
		return Path.GetHashCode();
	}

	public override string ToString()
	{
		return IsSet ? $"Ref<{typeof(T).Name}>({Path})" : $"Ref<{typeof(T).Name}>(none)";
	}
}
