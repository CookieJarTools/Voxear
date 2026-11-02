using Voxear.Interop;

namespace Voxear;

/// <summary>Frame timing. Every property reads the engine's own copy, so nothing is copied per frame.</summary>
public static unsafe class Time
{
	public static float Delta => Native.Api->Time->Delta;

	public static float UnscaledDelta => Native.Api->Time->UnscaledDelta;

	public static float FixedDelta => Native.Api->Time->FixedDelta;

	public static float TimeScale => Native.Api->Time->TimeScale;

	public static double Elapsed => Native.Api->Time->Elapsed;

	public static ulong Frame => Native.Api->Time->Frame;
}
