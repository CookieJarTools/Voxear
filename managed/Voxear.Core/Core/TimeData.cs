using System.Runtime.InteropServices;

namespace Voxear
{
	/// <summary>The engine's frame timing, read in place. Mirrors Time in crates/voxear_core/src/time.rs.</summary>
	[StructLayout(LayoutKind.Sequential)]
	internal struct TimeData
	{
		public float Delta;
		public float UnscaledDelta;
		public float FixedDelta;
		public float TimeScale;
		public double Elapsed;
		public ulong Frame;
	}
}
