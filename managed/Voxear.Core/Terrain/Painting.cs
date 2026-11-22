using System.Numerics;
using System.Text.Json.Nodes;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// Paints the level's world of blocks without end's land, height and biomes into its generator, a dab at a time, as
/// the Block World's Land, Height, Biome and Erase brushes do. The ground under a dab is made again with the
/// generator's own coasts, peaks and valleys.
/// <code>
/// // An island raised where the player stands.
/// Painting.Paint(PaintMap.Land, player.Position, radius: 120f, strength: 1f);
/// Painting.Paint(PaintMap.Height, player.Position, radius: 60f, strength: 0.5f);
/// </code>
/// </summary>
public static class Painting
{
	/// <summary>A dab round <paramref name="at"/>, <paramref name="radius"/> metres across; whether it changed anything.</summary>
	public static bool Paint(PaintMap map, Vector3 at, float radius = 40f, float strength = 0.3f, float falloff = 0.5f, PaintMode mode = PaintMode.Add, int biome = 0)
	{
		var asked = new JsonObject
		{
			["ask"] = "paint",
			["map"] = PostJson.Of(map),
			["at"] = PostJson.Of(at),
			["radius"] = radius,
			["strength"] = strength,
			["falloff"] = falloff,
			["mode"] = PostJson.Of(mode),
			["biome"] = biome,
		};
		return NativePost.Call(NativePost.WorldAsk, asked.ToJsonString()) == "true";
	}
}
