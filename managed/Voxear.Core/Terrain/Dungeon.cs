using System.Collections.Generic;
using System.Numerics;
using System.Text.Json;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A dungeon as a world of blocks lays it out: its entrance at the surface, its floors, its spaces from the first
/// floor's landing down to the boss's arena, the ways between them and its lights. Read from <see cref="Dungeons"/>.
/// A dungeon in a dimension of its own is only its gate in the land, with a Portal in it; inside its dimension,
/// Dungeons lists it whole.
/// <code>
/// var dungeon = Dungeons.Nearest(player.Position);
/// if (dungeon is { IsCleared: true } &amp;&amp; dungeon.Grow())
/// {
///     dungeon.IsCleared = false;   // a floor deeper, and its new boss waiting
/// }
/// </code>
/// </summary>
public sealed class Dungeon
{
	/// <summary>Its site as the engine reads it, to ask of its stage and its dimension by.</summary>
	private readonly string? site;
	private bool cleared;

	internal Dungeon(JsonElement json)
	{
		World = json.GetProperty("world").GetUInt64();
		Floors = json.GetProperty("floors").GetInt32();
		site = json.TryGetProperty("site", out var at) ? at.GetRawText() : null;
		Stage = json.TryGetProperty("stage", out var stage) ? stage.GetInt32() : 0;
		cleared = json.TryGetProperty("cleared", out var done) && done.GetBoolean();
		IsInside = json.TryGetProperty("inside", out var inside) && inside.GetBoolean();
		Dimension = json.TryGetProperty("dimension", out var made) && made.ValueKind == JsonValueKind.Number ? new DimensionInstance(made.GetUInt32()) : null;
		Portal = json.TryGetProperty("portal", out var portal) && portal.ValueKind == JsonValueKind.Number ? new Portal(new Obj(portal.GetUInt64())) : Portal.None;
		var entrance = json.GetProperty("entrance");
		Entrance = new DungeonEntrance(
			DungeonJson.Kind<DungeonEntranceKind>(entrance.GetProperty("kind")),
			DungeonJson.Vector(entrance.GetProperty("at")),
			DungeonJson.Vector(entrance.GetProperty("facing")));
		var spaces = new List<DungeonSpace>();
		foreach (var space in json.GetProperty("spaces").EnumerateArray())
		{
			spaces.Add(new DungeonSpace(this, space));
		}
		Spaces = spaces;
		var ways = new List<DungeonWay>();
		foreach (var way in json.GetProperty("ways").EnumerateArray())
		{
			ways.Add(new DungeonWay(this, way));
		}
		Ways = ways;
		var lights = new List<DungeonLight>();
		foreach (var light in json.GetProperty("lights").EnumerateArray())
		{
			lights.Add(new DungeonLight(DungeonJson.Kind<DungeonLightKind>(light.GetProperty("kind")), DungeonJson.Vector(light.GetProperty("at"))));
		}
		Lights = lights;
	}

	/// <summary>The Obj id of the world of blocks it is in: inside its own dimension, that dimension's world of rock.</summary>
	public ulong World { get; }

	/// <summary>
	/// How far it has grown: 0 as its site makes it, and a floor more under the last for each stage. Saved with the
	/// scene, beside its site.
	/// </summary>
	public int Stage { get; private set; }

	/// <summary>Whether it was cleared: a flag for the game's own rules, saved with the scene beside its stage.</summary>
	public bool IsCleared
	{
		get { return cleared; }
		set
		{
			cleared = value;
			if (site != null)
			{
				var flag = value ? "true" : "false";
				NativeDimensions.Tell(NativeDimensions.SetStageText, 0, $$"""{"site":{{site}},"cleared":{{flag}}}""");
			}
		}
	}

	/// <summary>Whether it was read inside its own dimension, its spaces in that dimension's coordinates.</summary>
	public bool IsInside { get; }

	/// <summary>Its own dimension, where it is made now; null where it is not, or the dungeon is in the land.</summary>
	public DimensionInstance? Dimension { get; }

	/// <summary>The Portal stood in its gate in the land, while one stands there; None otherwise, and inside.</summary>
	public Portal Portal { get; }

	/// <summary>How many floors it goes down.</summary>
	public int Floors { get; }

	/// <summary>Its entrance at the surface: the only part of it the land does not cover.</summary>
	public DungeonEntrance Entrance { get; }

	/// <summary>Its spaces: the first floor's landing first, then each floor's in the order of its graph.</summary>
	public IReadOnlyList<DungeonSpace> Spaces { get; }

	/// <summary>The ways between its spaces: the grand stair from the entrance, passages and shortcuts.</summary>
	public IReadOnlyList<DungeonWay> Ways { get; }

	/// <summary>Its braziers, crystals and the glows of its set pieces.</summary>
	public IReadOnlyList<DungeonLight> Lights { get; }

	/// <summary>The landing at the foot of the grand stair.</summary>
	public DungeonSpace? Start => Spaces.Count > 0 ? Spaces[0] : null;

	/// <summary>The boss's arena at the end of the last floor; null where it has none.</summary>
	public DungeonSpace? Boss
	{
		get
		{
			foreach (var space in Spaces)
			{
				if (space.Kind == DungeonSpaceKind.Arena)
				{
					return space;
				}
			}
			return null;
		}
	}

	/// <summary>Its spaces of <paramref name="kind"/>.</summary>
	public IEnumerable<DungeonSpace> SpacesOf(DungeonSpaceKind kind)
	{
		foreach (var space in Spaces)
		{
			if (space.Kind == kind)
			{
				yield return space;
			}
		}
	}

	/// <summary>
	/// Grows it a stage: a floor more under its last, whose arena becomes its way down, a new arena on the new floor.
	/// The floors above keep their layout and what was changed in them. Only while nobody is in its dimension, and
	/// not past the deepest a dungeon goes; false where it did not grow. Its dimension, made with nobody in it, is
	/// let go and made again grown when it is next needed.
	/// </summary>
	public bool Grow()
	{
		if (site == null || !NativeDimensions.Tell(NativeDimensions.SetStageText, 0, $$"""{"site":{{site}},"grow":true}"""))
		{
			return false;
		}
		Stage++;
		return true;
	}

	/// <summary>
	/// Makes its own dimension, or gives the one made already, without going near its gate: for a script that fills
	/// it before anybody arrives. It goes once nothing needs it, unless <paramref name="keepLoaded"/>.
	/// </summary>
	public DimensionMake MakeDimension(bool keepLoaded = false)
	{
		return Dimensions.MakeDungeon(site ?? "null", keepLoaded);
	}

	/// <summary>The space <paramref name="position"/> is inside; null where it is in none.</summary>
	public DungeonSpace? SpaceAt(Vector3 position)
	{
		foreach (var space in Spaces)
		{
			if (space.Contains(position))
			{
				return space;
			}
		}
		return null;
	}
}
