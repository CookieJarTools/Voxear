using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The biomes of the level's generated worlds of blocks: which kind of country a place is.
/// <code>
/// var biome = Biomes.At(player.Position);
/// if (biome.IsValid)
/// {
///     label.Text = biome.Name;
///     label.Color = biome.Color;
/// }
/// </code>
/// A place's biome is its ground's, however high over the ground or deep under it the place is. Where no world with
/// biomes reaches, it is <see cref="Biome.None"/>.
/// </summary>
public static class Biomes
{
	/// <summary>The biome the place at <paramref name="position"/> is; None where no world of blocks with biomes reaches.</summary>
	public static Biome At(Vector3 position)
	{
		return Biome.FromKey(NativeBiomes.At(position));
	}
}
