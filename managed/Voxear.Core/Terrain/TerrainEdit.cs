using System.Numerics;

namespace Voxear;

/// <summary>
/// One change to the ground as a value: what it does, where, how far it reaches and how much. Make one with the
/// methods here and apply it with <see cref="Terrain.Apply"/>, which the easy calls such as
/// <see cref="Terrain.Crater"/> do in one go. The same edits give the same ground on every machine, so a networked
/// game sends what makes an edit to every player in an RPC and each applies it.
/// <code>
/// [ClientRpc]
/// private void Exploded(Vector3 at, float size)
/// {
///     Terrain.Apply(TerrainEdit.Crater(at, size, size * 0.4f, Dirt));
/// }
/// </code>
/// </summary>
public readonly record struct TerrainEdit(TerrainEditKind Kind, Vector3 Point, float Radius, float Amount, MaterialAsset Surface = default, bool Cut = false, Vector3 To = default)
{
	/// <summary>Raises the ground within <paramref name="radius"/> metres by <paramref name="height"/> at the middle; less than nothing lowers it.</summary>
	public static TerrainEdit Raise(Vector3 point, float radius, float height)
	{
		return new TerrainEdit(TerrainEditKind.Raise, point, radius, height);
	}

	/// <summary>
	/// A crater <paramref name="radius"/> metres round and <paramref name="depth"/> deep, its rim thrown up round it,
	/// the grass and plants in it gone, and the surface drawn with <paramref name="walls"/> laid on its walls, when the
	/// terrain has one.
	/// </summary>
	public static TerrainEdit Crater(Vector3 point, float radius, float depth, MaterialAsset walls = default)
	{
		return new TerrainEdit(TerrainEditKind.Crater, point, radius, depth, walls);
	}

	/// <summary>Brings the ground within <paramref name="radius"/> metres to <paramref name="height"/> in the world.</summary>
	public static TerrainEdit Flatten(Vector3 point, float radius, float height)
	{
		return new TerrainEdit(TerrainEditKind.Flatten, point, radius, height);
	}

	/// <summary>Lays the terrain's surface drawn with <paramref name="surface"/> over the ground, as much as <paramref name="strength"/> at the middle.</summary>
	public static TerrainEdit Paint(Vector3 point, float radius, MaterialAsset surface, float strength = 1f)
	{
		return new TerrainEdit(TerrainEditKind.Paint, point, radius, strength, surface);
	}

	/// <summary>Cuts the ground away within <paramref name="radius"/> metres, or fills the holes there again.</summary>
	public static TerrainEdit Holes(Vector3 point, float radius, bool cut)
	{
		return new TerrainEdit(TerrainEditKind.Holes, point, radius, 0f, default, cut);
	}

	/// <summary>Brings the grass within <paramref name="radius"/> metres towards <paramref name="amount"/>: 0 clears it, 1 grows it fully.</summary>
	public static TerrainEdit Grass(Vector3 point, float radius, float amount)
	{
		return new TerrainEdit(TerrainEditKind.Grass, point, radius, amount);
	}

	/// <summary>Digs a ball of ground <paramref name="radius"/> metres round out of it, laying the surface drawn with <paramref name="walls"/> on what it bares.</summary>
	public static TerrainEdit Dig(Vector3 point, float radius, MaterialAsset walls = default)
	{
		return new TerrainEdit(TerrainEditKind.Dig, point, radius, 0f, walls);
	}

	/// <summary>Puts a ball of ground <paramref name="radius"/> metres round back.</summary>
	public static TerrainEdit Fill(Vector3 point, float radius)
	{
		return new TerrainEdit(TerrainEditKind.Fill, point, radius, 0f);
	}

	/// <summary>Digs a tunnel <paramref name="radius"/> metres round from <paramref name="from"/> to <paramref name="to"/>, rounded at its ends.</summary>
	public static TerrainEdit Tunnel(Vector3 from, Vector3 to, float radius, MaterialAsset walls = default)
	{
		return new TerrainEdit(TerrainEditKind.Tunnel, from, radius, 0f, walls, false, to);
	}

	/// <summary>Blows a crater <paramref name="radius"/> metres round out of the ground at <paramref name="point"/>, its lip thrown up.</summary>
	public static TerrainEdit Explode(Vector3 point, float radius, MaterialAsset walls = default)
	{
		return new TerrainEdit(TerrainEditKind.Explode, point, radius, 0f, walls);
	}
}
