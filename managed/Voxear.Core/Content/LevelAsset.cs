using System;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A level of the project, as a field holds it: picked in the Inspector, or dragged there from the Project panel.
/// Give it to Level.LoadAsync, or Level.Load, to change to the level. It names the level by its ID, so renaming or
/// moving the file keeps the field pointing at it, and nothing of the level is read until it is loaded.
/// <code>
/// public LevelAsset Next;
/// ...
/// await Level.LoadAsync(Next);
/// </code>
/// </summary>
public readonly struct LevelAsset : IEquatable<LevelAsset>, IAsset<LevelAsset>
{
	public static readonly LevelAsset None = default;

	private readonly string? reference;

	private LevelAsset(string reference)
	{
		this.reference = reference;
	}

	/// <summary>Whether it names a level.</summary>
	public bool IsSet => !string.IsNullOrEmpty(reference);

	/// <summary>The level's GUID or project path, as the scene or code gave it.</summary>
	public string Reference => reference ?? string.Empty;

	string? IAsset<LevelAsset>.AssetGuid => IsSet ? NativeSaves.Key(NativeSaves.KeyReference, 0, Reference) : null;

	/// <summary>
	/// The level at a project path or GUID, as "Levels/Night.level". Nothing is read: the level loads when it is
	/// asked for, so this costs nothing and never fails here.
	/// </summary>
	public static LevelAsset Load(string path)
	{
		return new LevelAsset(path);
	}

	public static bool operator ==(LevelAsset left, LevelAsset right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(LevelAsset left, LevelAsset right)
	{
		return !left.Equals(right);
	}

	public bool Equals(LevelAsset other)
	{
		return Reference == other.Reference;
	}

	public override bool Equals(object? other)
	{
		return other is LevelAsset asset && Equals(asset);
	}

	public override int GetHashCode()
	{
		return Reference.GetHashCode();
	}

	public override string ToString()
	{
		return IsSet ? Reference : "no level";
	}
}
