using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The level's baked lighting: how many lightmaps and light probes its bake has, the probes' light at any point,
/// and, in the editor, a bake under way. A level is baked with Bake Lighting, and opens with its bake.
/// <code>
/// if (BakedLighting.IsBaked)
/// {
///     var below = BakedLighting.ProbeLightAt(position).Irradiance(-Vector3.UnitY);
/// }
/// </code>
/// </summary>
public static class BakedLighting
{
	private const int LightmapCountValue = 3;
	private const int ProbeCountValue = 4;
	private const int BakingValue = 5;
	private const int AskAtValue = 6;
	private const int HarmonicValue = 10;

	/// <summary>Whether the level has a bake: lightmaps or light probes.</summary>
	public static bool IsBaked => LightmapCount > 0 || ProbeCount > 0;

	/// <summary>How many lightmaps the level's bake has.</summary>
	public static int LightmapCount => (int)Number(LightmapCountValue);

	/// <summary>How many light probes the level's bake has.</summary>
	public static int ProbeCount => (int)Number(ProbeCountValue);

	/// <summary>Whether the editor is baking the level's lighting now.</summary>
	public static bool IsBaking => Number(BakingValue) != 0f;

	/// <summary>How far the bake under way has gone, from 0 to 1; 0 when none is.</summary>
	public static float BakeProgress
	{
		get
		{
			var found = NativeComponent.Get(NativeComponent.BakedLighting, Obj.None, BakingValue, out var value);
			return found ? value.Y : 0f;
		}
	}

	/// <summary>
	/// The baked light probes' light at <paramref name="position"/>, in the world, blended from the probes round it
	/// as an Obj there is lit by them. Not valid where the level has no probes.
	/// </summary>
	public static ProbeLight ProbeLightAt(Vector3 position)
	{
		if (!NativeComponent.Set(NativeComponent.BakedLighting, Obj.None, AskAtValue, new Vector4(position, 0f)))
		{
			return default;
		}
		var harmonics = new Vector3[9];
		for (var i = 0; i < 9; i++)
		{
			if (!NativeComponent.Get(NativeComponent.BakedLighting, Obj.None, HarmonicValue + i, out var value))
			{
				return default;
			}
			harmonics[i] = new Vector3(value.X, value.Y, value.Z);
		}
		return new ProbeLight(harmonics);
	}

	private static float Number(int what)
	{
		return NativeComponent.Number(NativeComponent.BakedLighting, Obj.None, what, 0f);
	}
}
