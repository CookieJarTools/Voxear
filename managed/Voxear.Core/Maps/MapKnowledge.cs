using System;
using System.Globalization;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// What one map knows of the world: the ground it has seen, and what it last heard of each place and when. It is
/// kept apart from the world, so each player can have a map of their own that shows what that player saw and was
/// told, however things stand now. Nothing in it changes unless a script changes it.
/// <code>
/// // The maps show only what scripts reveal.
/// map.Shows = MapShows.GameDecides;
///
/// var known = MapKnowledge.Shown;
/// known.Reveal(player.WorldPosition, 120f);
/// known.Paint(Threat, dungeon.WorldPosition, 150f, 0.8f, today);
///
/// // Saved with the player, and loaded again.
/// file.Set("map", known.ToBytes());
/// known.Load(file.Get&lt;byte[]&gt;("map"));
/// </code>
/// <para>
/// A host keeps one for each player with <c>new MapKnowledge()</c>, sends a player what they just learnt with
/// ChangesSince, and the player's game takes it in with Merge. Whether one player's reading of a board updates the
/// whole group's maps is a Merge into each, or not.
/// </para>
/// </summary>
public sealed class MapKnowledge
{
	private const int NewText = 200;
	private const int DropText = 201;
	private const int ShowText = 202;
	private const int RevealText = 204;
	private const int AllText = 205;
	private const int ForgetText = 206;
	private const int KnowsText = 207;
	private const int PaintText = 208;
	private const int NewsText = 209;
	private const int BytesText = 210;
	private const int LoadText = 211;
	private const int MergeText = 212;
	private const int ChangesText = 213;
	private const int StampText = 214;
	private const int PaperText = 216;
	private const int ExploreText = 217;
	private const int JoinText = 218;

	private static readonly MapKnowledge First = new MapKnowledge(0);

	private static uint made;
	private static MapKnowledge shown = First;

	private readonly uint id;

	/// <summary>A map that knows nothing yet.</summary>
	public MapKnowledge()
	{
		made++;
		id = made;
		Tell(NewText, string.Empty);
	}

	private MapKnowledge(uint id)
	{
		this.id = id;
	}

	/// <summary>
	/// The knowledge the maps show: this machine's player's, to begin with one that knows nothing. Set another to
	/// show that in its place, as when looking at a friend's map.
	/// </summary>
	public static MapKnowledge Shown
	{
		get { return shown; }
		set
		{
			shown = value;
			value.Tell(ShowText, string.Empty);
		}
	}

	/// <summary>The colour of ground the maps do not know.</summary>
	public static Color Paper
	{
		set { NativeComponent.Tell(NativeComponent.Map, Obj.None, PaperText, Numbers(value.R, value.G, value.B)); }
	}

	/// <summary>How far round what the maps follow ground is seen, in metres, when they show what was explored.</summary>
	public static float ExploreRadius
	{
		set { NativeComponent.Tell(NativeComponent.Map, Obj.None, ExploreText, Numbers(value)); }
	}

	/// <summary>Counted up with each change: keep it to ask later what changed since, with ChangesSince.</summary>
	public uint Stamp => uint.TryParse(Ask(StampText, string.Empty), NumberStyles.Integer, CultureInfo.InvariantCulture, out var stamp) ? stamp : 0;

	/// <summary>Sees the ground within <paramref name="radius"/> metres of a place. True if any of it was new.</summary>
	public bool Reveal(Vector3 position, float radius)
	{
		return Ask(RevealText, Numbers(position.X, position.Z, radius)) == "1";
	}

	/// <summary>Sees all the ground there is.</summary>
	public void RevealAll()
	{
		Tell(AllText, "1");
	}

	/// <summary>Forgets everything: the ground seen and all news.</summary>
	public void Forget()
	{
		Tell(AllText, "0");
		Tell(ForgetText, string.Empty);
	}

	/// <summary>Whether the ground at a place is seen.</summary>
	public bool Knows(Vector3 position)
	{
		return Ask(KnowsText, Numbers(position.X, position.Z)) == "1";
	}

	/// <summary>
	/// Hears, at time <paramref name="heard"/>, that a layer is <paramref name="value"/>, from 0 to 1, within
	/// <paramref name="radius"/> metres of a place, in place of what was known there. The time is the game's own:
	/// days played, or seconds, anything that only grows.
	/// </summary>
	public void Paint(MapLayer layer, Vector3 position, float radius, float value, uint heard = 0)
	{
		Tell(PaintText, string.Create(CultureInfo.InvariantCulture, $"{layer.Id} {position.X} {position.Z} {radius} {value} {heard}"));
	}

	/// <summary>What was last heard of a layer at a place, from 0 to 1: null where nothing was.</summary>
	public float? News(MapLayer layer, Vector3 position)
	{
		return Heard(layer, position)?.Value;
	}

	/// <summary>When the news of a layer at a place was heard, as Paint was told: null where nothing was.</summary>
	public uint? NewsHeard(MapLayer layer, Vector3 position)
	{
		return Heard(layer, position)?.When;
	}

	/// <summary>All of it as a few bytes, to save or to send.</summary>
	public byte[] ToBytes()
	{
		return Convert.FromBase64String(Ask(BytesText, string.Empty) ?? string.Empty);
	}

	/// <summary>What changed after a Stamp, as bytes another map takes in with Merge: what a player just learnt, not their whole map.</summary>
	public byte[] ChangesSince(uint stamp)
	{
		return Convert.FromBase64String(Ask(ChangesText, stamp.ToString(CultureInfo.InvariantCulture)) ?? string.Empty);
	}

	/// <summary>Becomes what ToBytes gave: what was saved, and no more. False when the bytes are not a map's.</summary>
	public bool Load(byte[] bytes)
	{
		return Ask(LoadText, Convert.ToBase64String(bytes)) != null;
	}

	/// <summary>
	/// Takes in what ToBytes or ChangesSince gave: ground either has seen is seen, and of two pieces of news of a
	/// place the one heard later is kept. False when the bytes are not a map's.
	/// </summary>
	public bool Merge(byte[] bytes)
	{
		return Ask(MergeText, Convert.ToBase64String(bytes)) != null;
	}

	/// <summary>Takes in what another map knows, as Merge does its bytes.</summary>
	public void Merge(MapKnowledge other)
	{
		Tell(JoinText, other.id.ToString(CultureInfo.InvariantCulture));
	}

	/// <summary>Lets the engine forget it, when the player it was for has gone. The one shown stays.</summary>
	public void Dispose()
	{
		Tell(DropText, string.Empty);
	}

	private (float Value, uint When)? Heard(MapLayer layer, Vector3 position)
	{
		var answer = Ask(NewsText, Numbers(layer.Id, position.X, position.Z));
		var words = (answer ?? string.Empty).Split(' ');
		if (words.Length != 2)
		{
			return null;
		}
		var value = float.Parse(words[0], CultureInfo.InvariantCulture);
		return (value, (uint)float.Parse(words[1], CultureInfo.InvariantCulture));
	}

	private static string Numbers(params float[] numbers)
	{
		return string.Join(' ', Array.ConvertAll(numbers, number => number.ToString(CultureInfo.InvariantCulture)));
	}

	private string? Ask(int what, string text)
	{
		var whole = text.Length == 0 ? id.ToString(CultureInfo.InvariantCulture) : $"{id.ToString(CultureInfo.InvariantCulture)} {text}";
		return NativeComponent.Ask(NativeComponent.Map, Obj.None, what, whole);
	}

	private void Tell(int what, string text)
	{
		Ask(what, text);
	}
}
