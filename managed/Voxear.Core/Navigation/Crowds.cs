using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The crowd as a whole: how far its tiers reach, and how many characters are in each. Project Settings ▸ Crowds sets
/// the distances; a game may change them as it plays, for a quality setting or a busy scene.
/// <code>
/// // Fewer characters near on a slow machine.
/// Crowds.Near = 25f;
/// Log.Info($"{Crowds.Counts.Near} near, {Crowds.Counts.Records} kept as records");
/// </code>
/// </summary>
public static class Crowds
{
	private const int CountsValue = 0;
	private const int DistancesValue = 1;
	private const int EnabledValue = 2;

	/// <summary>How many characters are near, middle and far this frame, and how many are kept as records.</summary>
	public static CrowdCounts Counts
	{
		get
		{
			NativeComponent.Get(NativeComponent.Crowds, Obj.None, CountsValue, out var counts);
			return new CrowdCounts((int)counts.X, (int)counts.Y, (int)counts.Z, (int)counts.W);
		}
	}

	/// <summary>Whether characters further off are done less often; off, every one is near.</summary>
	public static bool Enabled
	{
		get { return NativeComponent.Number(NativeComponent.Crowds, Obj.None, EnabledValue, 1f) != 0f; }
		set { NativeComponent.Set(NativeComponent.Crowds, Obj.None, EnabledValue, new Vector4(value ? 1f : 0f, 0f, 0f, 0f)); }
	}

	/// <summary>Within this many metres of the camera or a player, a character is near.</summary>
	public static float Near
	{
		get { return Distances().X; }
		set { SetDistances(Distances() with { X = value }); }
	}

	/// <summary>Out to this many metres a character is in the middle tier; beyond it, far.</summary>
	public static float Middle
	{
		get { return Distances().Y; }
		set { SetDistances(Distances() with { Y = value }); }
	}

	/// <summary>Beyond this many metres a Crowd Member kept as a record goes as a record; 0 keeps every one an Obj.</summary>
	public static float RecordsBeyond
	{
		get { return Distances().Z; }
		set { SetDistances(Distances() with { Z = value }); }
	}

	private static Vector4 Distances()
	{
		return NativeComponent.Get(NativeComponent.Crowds, Obj.None, DistancesValue, out var distances)
			? distances
			: new Vector4(40f, 150f, 250f, 0f);
	}

	private static void SetDistances(Vector4 distances)
	{
		NativeComponent.Set(NativeComponent.Crowds, Obj.None, DistancesValue, distances);
	}
}
