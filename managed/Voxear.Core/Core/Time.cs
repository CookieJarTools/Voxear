using Voxear.Interop;

namespace Voxear;

/// <summary>Frame timing. Every property reads the engine's own copy, so nothing is copied per frame.</summary>
public static unsafe class Time
{
	public static float Delta => Native.Api->Time->Delta;

	public static float UnscaledDelta => Native.Api->Time->UnscaledDelta;

	public static float FixedDelta => Native.Api->Time->FixedDelta;

	/// <summary>
	/// How fast the game's time goes: 1 as fast as real time, 0.5 half as fast, 0 stopped, at most 100. Delta and the
	/// fixed steps go by it, UnscaledDelta does not. Set, it takes effect from the next frame; Project Settings ▸ Time
	/// sets what it starts at.
	/// </summary>
	public static float TimeScale
	{
		get => Native.Api->Time->TimeScale;
		set => ProjectSettings.ChangeTimeScale(value);
	}

	public static double Elapsed => Native.Api->Time->Elapsed;

	public static ulong Frame => Native.Api->Time->Frame;
}
