using System;
using System.Numerics;
using System.Text;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The level's ground: how high it is under a point, which way it faces there, and where a ray meets it, read from
/// every Terrain's heights. It answers with or without physics, the same on every machine. Where no terrain reaches,
/// or in a hole cut in one, there is no ground: <see cref="HeightAt"/> gives negative infinity.
/// <code>
/// protected override void Update()
/// {
///     // Keep a marker on the ground as it moves about.
///     var at = Transform.Position;
///     Transform.Position = new Vector3(at.X, Terrain.HeightAt(at), at.Z);
/// }
/// </code>
/// Scripts change the ground too: a shell's crater, a trench dug lower, a hole cut, mud painted, grass cleared. Each
/// change gives the same ground on every machine, and physics and drawing follow it at once. A save of the scene
/// keeps them, and <see cref="CaptureEdits"/> gives them to a player joining a game.
/// <code>
/// Terrain.Crater(hit.Point, 3f, 1.2f, Dirt);
/// </code>
/// </summary>
public static unsafe class Terrain
{
	/// <summary>Whether the level has a terrain.</summary>
	public static bool Exists => Native.Api->TerrainCount() > 0;

	/// <summary>How high the ground is under <paramref name="position"/>, or negative infinity off every terrain or in a hole.</summary>
	public static float HeightAt(Vector3 position)
	{
		return TryGetGround(position, out var ground) ? ground.Height : float.NegativeInfinity;
	}

	/// <summary>The way the ground faces under <paramref name="position"/>, smoothed as it is drawn; straight up off it.</summary>
	public static Vector3 NormalAt(Vector3 position)
	{
		return TryGetGround(position, out var ground) ? ground.Normal : Vector3.UnitY;
	}

	/// <summary>The ground under <paramref name="position"/> in one go: its height, its normal and its terrain's Obj.</summary>
	public static bool TryGetGround(Vector3 position, out TerrainGround ground)
	{
		var values = stackalloc float[4];
		ulong obj;
		if (Native.Api->TerrainGround(&position, values, &obj) == 0)
		{
			ground = default;
			return false;
		}
		ground = new TerrainGround(values[0], new Vector3(values[1], values[2], values[3]), new Obj(obj));
		return true;
	}

	/// <summary>
	/// The first place a ray from <paramref name="origin"/> along <paramref name="direction"/> meets the ground
	/// within <paramref name="maxDistance"/> metres, from above or below. Only terrains count: for anything that
	/// collides, terrains among it, use <see cref="Physics.Raycast(Vector3, Vector3, out RaycastHit, float, int, bool)"/>.
	/// </summary>
	public static bool Raycast(Vector3 origin, Vector3 direction, out RaycastHit hit, float maxDistance = float.PositiveInfinity)
	{
		RaycastHitData found;
		if (Native.Api->TerrainRaycast(&origin, &direction, maxDistance, &found) == 0)
		{
			hit = default;
			return false;
		}
		var point = new Vector3(found.PointX, found.PointY, found.PointZ);
		var normal = new Vector3(found.NormalX, found.NormalY, found.NormalZ);
		hit = new RaycastHit(new Obj(found.Obj), point, normal, found.Distance);
		return true;
	}

	/// <summary>Raises the ground within <paramref name="radius"/> metres of <paramref name="point"/> by <paramref name="height"/> at the middle, easing to nothing at the edge. Whether any terrain reached.</summary>
	public static bool Raise(Vector3 point, float radius, float height)
	{
		return Apply(TerrainEdit.Raise(point, radius, height));
	}

	/// <summary>Lowers the ground within <paramref name="radius"/> metres of <paramref name="point"/> by <paramref name="depth"/> at the middle.</summary>
	public static bool Lower(Vector3 point, float radius, float depth)
	{
		return Apply(TerrainEdit.Raise(point, radius, -depth));
	}

	/// <summary>Brings the ground within <paramref name="radius"/> metres of <paramref name="point"/> to <paramref name="height"/> in the world: a floor for a trench or a gun pit.</summary>
	public static bool Flatten(Vector3 point, float radius, float height)
	{
		return Apply(TerrainEdit.Flatten(point, radius, height));
	}

	/// <summary>
	/// Blows a crater <paramref name="radius"/> metres round and <paramref name="depth"/> deep into the ground at
	/// <paramref name="point"/>, its rim thrown up round it, its grass cleared and the plants in it taken away, with the
	/// terrain's surface drawn with <paramref name="walls"/> laid on its walls when it has one.
	/// </summary>
	public static bool Crater(Vector3 point, float radius, float depth, MaterialAsset walls = default)
	{
		return Apply(TerrainEdit.Crater(point, radius, depth, walls));
	}

	/// <summary>Lays the terrain's surface drawn with <paramref name="surface"/> over the ground within <paramref name="radius"/> metres, as much as <paramref name="strength"/>, 0 to 1, at the middle.</summary>
	public static bool Paint(Vector3 point, float radius, MaterialAsset surface, float strength = 1f)
	{
		return Apply(TerrainEdit.Paint(point, radius, surface, strength));
	}

	/// <summary>Cuts the ground away within <paramref name="radius"/> metres of <paramref name="point"/>: things fall through, and rays pass.</summary>
	public static bool CutHole(Vector3 point, float radius)
	{
		return Apply(TerrainEdit.Holes(point, radius, true));
	}

	/// <summary>Fills the holes within <paramref name="radius"/> metres of <paramref name="point"/> again.</summary>
	public static bool FillHole(Vector3 point, float radius)
	{
		return Apply(TerrainEdit.Holes(point, radius, false));
	}

	/// <summary>Clears every kind of grass within <paramref name="radius"/> metres of <paramref name="point"/>.</summary>
	public static bool ClearGrass(Vector3 point, float radius)
	{
		return Apply(TerrainEdit.Grass(point, radius, 0f));
	}

	/// <summary>Grows every kind of grass within <paramref name="radius"/> metres of <paramref name="point"/> towards <paramref name="amount"/>, 1 for fully.</summary>
	public static bool GrowGrass(Vector3 point, float radius, float amount = 1f)
	{
		return Apply(TerrainEdit.Grass(point, radius, amount));
	}

	/// <summary>
	/// Digs a ball of ground <paramref name="radius"/> metres round out of the terrain at <paramref name="point"/>,
	/// up and down as well as across, so it can undercut and overhang as heights cannot: a dugout, a shell hole's
	/// steep sides. The surface drawn with <paramref name="walls"/> is laid on what it bares, and what stood on ground it
	/// took away comes down.
	/// </summary>
	public static bool Dig(Vector3 point, float radius, MaterialAsset walls = default)
	{
		return Apply(TerrainEdit.Dig(point, radius, walls));
	}

	/// <summary>Puts a ball of ground <paramref name="radius"/> metres round back at <paramref name="point"/>.</summary>
	public static bool Fill(Vector3 point, float radius)
	{
		return Apply(TerrainEdit.Fill(point, radius));
	}

	/// <summary>
	/// Digs a tunnel <paramref name="radius"/> metres round from <paramref name="from"/> to <paramref name="to"/>,
	/// rounded at its ends: along the ground a trench, under it a way through.
	/// </summary>
	public static bool Tunnel(Vector3 from, Vector3 to, float radius, MaterialAsset walls = default)
	{
		return Apply(TerrainEdit.Tunnel(from, to, radius, walls));
	}

	/// <summary>
	/// Blows a crater out of the ground at <paramref name="point"/>: a bowl <paramref name="radius"/> metres round, its
	/// middle a third of that under the point, with the earth thrown up round its lip, the surface drawn with
	/// <paramref name="walls"/> laid on what it bares, and the grass and plants it took away gone.
	/// </summary>
	public static bool Explode(Vector3 point, float radius, MaterialAsset walls = default)
	{
		return Apply(TerrainEdit.Explode(point, radius, walls));
	}

	/// <summary>Makes an edit to every terrain it reaches. Whether it reached any.</summary>
	public static bool Apply(TerrainEdit edit)
	{
		var data = new TerrainEditData
		{
			Kind = (int)edit.Kind,
			Cut = edit.Cut ? 1 : 0,
			At = edit.Point,
			Radius = edit.Radius,
			Amount = edit.Amount,
			To = edit.To,
		};
		var surface = Encoding.UTF8.GetBytes(edit.Surface.Reference);
		fixed (byte* named = surface)
		{
			return Native.Api->TerrainEdit(&data, named, surface.Length) > 0;
		}
	}

	/// <summary>
	/// Every change scripts made to the level's terrains, as few bytes as say it: each terrain's edits one by one
	/// while they are few, or the tiles they changed once those are smaller. A save of the scene keeps them; a game
	/// sends them to a player who joins, who gives them to <see cref="ApplyEdits"/>.
	/// </summary>
	public static byte[] CaptureEdits()
	{
		if (!Native.IsConnected)
		{
			return [];
		}

		var length = Native.Api->TerrainEdits(0, null, 0, null, 0);
		while (true)
		{
			var buffer = new byte[Math.Max(length, 0)];
			fixed (byte* into = buffer)
			{
				length = Native.Api->TerrainEdits(0, null, 0, into, buffer.Length);
			}
			if (length <= buffer.Length)
			{
				return length == buffer.Length ? buffer : buffer[..length];
			}
		}
	}

	/// <summary>
	/// Puts each terrain <paramref name="edits"/> (from <see cref="CaptureEdits"/>) names back as it was built, then
	/// makes its changes. Whether they were edits; terrains this level has not are left out.
	/// </summary>
	public static bool ApplyEdits(byte[] edits)
	{
		if (!Native.IsConnected || edits.Length == 0)
		{
			return Native.IsConnected;
		}

		fixed (byte* from = edits)
		{
			return Native.Api->TerrainEdits(1, from, edits.Length, null, 0) >= 0;
		}
	}
}
