namespace Voxear.Interop;

/// <summary>
/// How plants move, read and set by number, as the engine's plants.rs numbers them. Without the engine, as in tests,
/// nothing is set and every number reads as NaN.
/// </summary>
internal static unsafe class NativeFoliage
{
	public const int Wind = 0;
	public const int Bend = 1;
	public const int Interaction = 2;
	public const int Push = 3;
	public const int Spring = 4;
	public const int BlowingX = 5;
	public const int BlowingZ = 6;
	public const int Blowing = 7;

	public static float Get(int which)
	{
		return Native.IsConnected ? Native.Api->FoliageMotion(which) : float.NaN;
	}

	public static void Set(int which, float value)
	{
		if (Native.IsConnected)
		{
			Native.Api->SetFoliageMotion(which, value);
		}
	}
}
