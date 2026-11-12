namespace Voxear.Interop;

/// <summary>
/// What making worlds of blocks may spend, read and set by number. Without the engine, as in tests, nothing is set
/// and every number reads as NaN.
/// </summary>
internal static unsafe class NativeWorldBudget
{
	public const int Quality = 0;
	public const int Threads = 1;
	public const int Columns = 2;
	public const int Upload = 3;
	public const int ViewDistance = 4;
	public const int FarDistance = 5;

	public static float Get(int which)
	{
		return Native.IsConnected ? Native.Api->WorldBudget(which) : float.NaN;
	}

	public static void Set(int which, float value)
	{
		if (Native.IsConnected)
		{
			Native.Api->SetWorldBudget(which, value);
		}
	}
}
