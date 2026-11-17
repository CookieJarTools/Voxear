using System;
using System.Numerics;
using System.Text.Json;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A dimension the game has made: the level's own, one made from a <see cref="Dimension"/> asset, or a dungeon's.
/// Read where an Obj is with <c>obj.Dimension</c> and where the game is seen from with Dimension.Current; make one
/// with Dimensions.Make. It has its own Blocks, Physics, Weather, Sky and Clock, the same calls as the static ones,
/// for a script that works in a dimension the game is not seen from.
/// <code>
/// // A town's smiths keep working while the player is down a dungeon.
/// var town = Dimensions.Made[0];
/// if (town.Blocks.At(forge) == 0)
/// {
///     town.Blocks.Set(forge, anvil);
/// }
/// </code>
/// A handle stays true only while its dimension is made: once let go, IsMade is false, and a dimension made later
/// is never taken for it.
/// </summary>
public readonly struct DimensionInstance : IEquatable<DimensionInstance>
{
	/// <summary>The level's own dimension: its scenes' Objs, and every Obj not put in another.</summary>
	public static readonly DimensionInstance Level = default;

	internal DimensionInstance(uint handle)
	{
		Handle = handle;
	}

	/// <summary>Whether it is the level's own dimension.</summary>
	public bool IsLevel => Handle == 0;

	/// <summary>Whether it is still made. The level's always is.</summary>
	public bool IsMade => IsLevel || NativeDimensions.IsMade(Handle);

	/// <summary>The <c>.dimension</c> it was made from; None for the level's, and for one made by code, as a dungeon's is.</summary>
	public Dimension Asset => Info("asset") is { ValueKind: JsonValueKind.String } asset ? Dimension.Load(asset.GetString()!) : Dimension.None;

	/// <summary>Which of its asset's dimensions it is: 0 where there is only one; for a dungeon's, a number of its site.</summary>
	public ulong Key => Info("key") is { ValueKind: JsonValueKind.Number } key ? key.GetUInt64() : 0;

	/// <summary>What it is for, as its file says: open land, a dungeon or caves. The level's is open land.</summary>
	public DimensionKind Kind => Info("kind") is { ValueKind: JsonValueKind.String } kind && Enum.TryParse<DimensionKind>(kind.GetString(), out var parsed) ? parsed : DimensionKind.Overworld;

	/// <summary>
	/// Whether it stays made until LetGo, or goes once nothing has needed it for its file's Keep Loaded seconds: no
	/// camera in it and no Portal near enough to show it. One a script makes stays until set otherwise; one a
	/// Portal or a dungeon's gate made goes.
	/// </summary>
	public bool KeepLoaded
	{
		get { return IsLevel || NativeDimensions.KeepLoaded(Handle); }
		set
		{
			if (!IsLevel)
			{
				NativeDimensions.SetKeepLoaded(Handle, value);
			}
		}
	}

	/// <summary>Its blocks: the same calls as <see cref="Voxear.Blocks"/>, asked of this dimension's worlds.</summary>
	public DimensionBlocks Blocks => new(this);

	/// <summary>Its physics: the same rays as <see cref="Voxear.Physics"/>, meeting only this dimension's colliders.</summary>
	public DimensionPhysics Physics => new(this);

	/// <summary>Its weather as it is now: clear in one that has none.</summary>
	public DimensionWeather Weather => new(this);

	/// <summary>Its sky: none in an enclosed dimension, as a dungeon is.</summary>
	public DimensionSky Sky => new(this);

	/// <summary>Its clock: the level's time of day, its own, or none.</summary>
	public DimensionClock Clock => new(this);

	/// <summary>The handle the engine knows it by.</summary>
	internal uint Handle { get; }

	/// <summary>Whether its ground is made within <paramref name="radius"/> metres of a place: at once in a world of its blocks alone.</summary>
	public bool IsReadyAround(Vector3 position, float radius = 16f)
	{
		return IsMade && NativeDimensions.IsReady(Handle, position, radius);
	}

	/// <summary>Waits until its ground is made round a place, for something to stand on it.</summary>
	public DimensionReady WhenReady(Vector3 position, float radius = 16f)
	{
		return new DimensionReady(this, position, radius);
	}

	/// <summary>
	/// Until the scope is disposed, every call that takes a place asks this dimension: Blocks, Physics, Water,
	/// Biomes and the rest, as though the game were seen from here.
	/// <code>
	/// using (dungeon.Use())
	/// {
	///     var floor = Blocks.FloorHeight(spawn);
	///     var clear = !Physics.Raycast(spawn, Vector3.UnitY, 3f);
	/// }
	/// </code>
	/// </summary>
	public DimensionScope Use()
	{
		return new DimensionScope(this);
	}

	/// <summary>
	/// Lets it go: its Objs are destroyed at the end of the frame, but persistent ones, which go to the level's. What
	/// was changed in it is kept, and is there again when it is made again. The level's cannot be let go.
	/// </summary>
	public void LetGo()
	{
		if (!IsLevel)
		{
			NativeDimensions.Tell(NativeDimensions.LetGoText, Handle, string.Empty);
		}
	}

	/// <summary>The dungeon's site, for a dungeon's own dimension, as the engine reads it; null for any other.</summary>
	internal JsonElement? DungeonSite => Info("dungeon") is { ValueKind: JsonValueKind.Object } site ? site : null;

	/// <summary>What the engine says of it under <paramref name="name"/>, or null.</summary>
	private JsonElement? Info(string name)
	{
		var json = NativeDimensions.Text(NativeDimensions.InfoText, Handle);
		if (json == null)
		{
			return null;
		}
		using var document = JsonDocument.Parse(json);
		return document.RootElement.TryGetProperty(name, out var value) ? value.Clone() : null;
	}

	public static bool operator ==(DimensionInstance left, DimensionInstance right)
	{
		return left.Handle == right.Handle;
	}

	public static bool operator !=(DimensionInstance left, DimensionInstance right)
	{
		return left.Handle != right.Handle;
	}

	public bool Equals(DimensionInstance other)
	{
		return Handle == other.Handle;
	}

	public override bool Equals(object? other)
	{
		return other is DimensionInstance dimension && Equals(dimension);
	}

	public override int GetHashCode()
	{
		return Handle.GetHashCode();
	}

	public override string ToString()
	{
		if (IsLevel)
		{
			return "the level's dimension";
		}
		return Asset.IsSet ? $"{Asset} ({Key})" : $"{Kind} dimension {Handle & 0xffff}";
	}
}
