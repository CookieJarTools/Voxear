using System.Numerics;

namespace Voxear.Tests;

/// <summary>The Reverb Probe and ray-traced audio APIs with no engine behind them: nothing to read, nothing throws.</summary>
internal static class ReverbProbeTests
{
	[Test]
	private static void AnObjWithoutAReverbProbeReadsAsNoneAndTakesChangesQuietly()
	{
		var probe = Obj.None.ReverbProbe;
		Check.True(!probe.IsValid && probe == ReverbProbe.None, "no Reverb Probe on None");
		probe.Size = new Vector3(10f, 4f, 10f);
		probe.Spacing = 2f;
		Check.Near(0f, probe.Spacing);
		Check.True(Obj.None.AddReverbProbe() == ReverbProbe.None, "nothing to add it to");
	}

	[Test]
	private static void RayTracedAudioReadsAsOffWithNoEngine()
	{
		Audio.RayTraced = true;
		Check.True(!Audio.RayTracing && Audio.Rays == 0, "nothing traced");
	}
}
