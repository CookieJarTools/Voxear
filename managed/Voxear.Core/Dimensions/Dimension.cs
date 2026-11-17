using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A <c>.dimension</c> of the project, as a field holds it: picked in the Inspector, or dragged there from the
/// Project panel. A dimension is a world of its own beside the level's, with its own Block Worlds, sky, sun and
/// weather, at coordinates that may be the same as the level's. Nothing of it is read until it is made, by a
/// Portal leading into it, by <see cref="Dimensions.Make"/>, or by moving an Obj into it.
/// <code>
/// public Ref&lt;Dimension&gt; Caves;
/// ...
/// // Into the caves, standing where their own coordinates say.
/// await player.MoveTo(Caves.Load(), new Vector3(0f, 12f, 0f));
///
/// Dimension.Entered += dimension =>
/// {
///     music.Play(dimension.Kind == DimensionKind.Overworld ? dayTheme : caveTheme);
/// };
/// </code>
/// Blocks, Physics, Weather and the other calls that take a place ask the dimension the game is seen from,
/// <see cref="Current"/>, so a game played in one dimension at a time needs nothing more. To ask of another, use
/// that dimension's own: <c>dimension.Blocks.At(position)</c>.
/// </summary>
public readonly struct Dimension : IEquatable<Dimension>, IAsset<Dimension>
{
	public static readonly Dimension None = default;

	private static uint current;
	private static bool started;

	private readonly string? reference;

	private Dimension(string reference)
	{
		this.reference = reference;
	}

	/// <summary>Raised when the game comes to be seen from another dimension: through a Portal, or moved there.</summary>
	public static event Action<DimensionInstance>? Entered;

	/// <summary>Raised for the dimension the game was seen from as it comes to be seen from another, before Entered.</summary>
	public static event Action<DimensionInstance>? Left;

	/// <summary>
	/// The dimension the game is seen from: the one the local player, or the first camera, is in. The level's own,
	/// <see cref="DimensionInstance.Level"/>, until something goes into another.
	/// </summary>
	public static DimensionInstance Current => new(NativeDimensions.Current());

	/// <summary>Whether it names a dimension.</summary>
	public bool IsSet => !string.IsNullOrEmpty(reference);

	/// <summary>The dimension's GUID or project path, as the scene or code gave it.</summary>
	public string Reference => reference ?? string.Empty;

	string? IAsset<Dimension>.AssetGuid => Guid;

	/// <summary>Its GUID, as the engine knows it; null away from the engine.</summary>
	internal string? Guid => IsSet ? NativeSaves.Key(NativeSaves.KeyReference, 0, Reference) : null;

	/// <summary>
	/// The dimension at a project path or GUID, as "Dimensions/Caves.dimension". Nothing is read: the dimension is
	/// made when something asks for it, so this costs nothing and never fails here.
	/// </summary>
	public static Dimension Load(string path)
	{
		return new Dimension(path);
	}

	/// <summary>
	/// This dimension as it is made now, where it is: the one made as <paramref name="key"/>, 0 for a dimension of
	/// which there is only one. Null where it is not made.
	/// </summary>
	public DimensionInstance? Made(ulong key = 0)
	{
		var self = this;
		foreach (var made in Dimensions.Made)
		{
			if (made.Asset == self && made.Key == key)
			{
				return made;
			}
		}
		return null;
	}

	/// <summary>Raises Left and Entered when the dimension the game is seen from has changed since the last frame.</summary>
	internal static void Dispatch()
	{
		var now = NativeDimensions.Current();
		if (!started)
		{
			(started, current) = (true, now);
			return;
		}
		if (now == current)
		{
			return;
		}
		var was = current;
		current = now;
		Left?.Invoke(new DimensionInstance(was));
		Entered?.Invoke(new DimensionInstance(now));
	}

	/// <summary>Moves an Obj into this dimension's one made as <paramref name="key"/>, made first if need be.</summary>
	internal DimensionMove Move(Obj obj, Vector3 position, ulong key)
	{
		return new DimensionMove(obj, Dimensions.Make(this, key), position);
	}

	public static bool operator ==(Dimension left, Dimension right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(Dimension left, Dimension right)
	{
		return !left.Equals(right);
	}

	/// <summary>Two name the same dimension when they name the same file, whether by its path or its GUID.</summary>
	public bool Equals(Dimension other)
	{
		return Reference == other.Reference || (IsSet && other.IsSet && Guid != null && Guid == other.Guid);
	}

	public override bool Equals(object? other)
	{
		return other is Dimension dimension && Equals(dimension);
	}

	public override int GetHashCode()
	{
		return (Guid ?? Reference).GetHashCode();
	}

	public override string ToString()
	{
		return IsSet ? Reference : "no dimension";
	}
}
