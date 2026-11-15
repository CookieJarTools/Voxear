namespace Voxear.Tests;

/// <summary>The Reflection Probe API with no engine behind it: no Obj has one, and nothing throws.</summary>
internal static class ReflectionProbeTests
{
	[Test]
	private static void AnObjWithoutAProbeReadsAsNoneAndTakesChangesQuietly()
	{
		var probe = Obj.None.ReflectionProbe;
		Check.True(!probe.IsValid && probe == ReflectionProbe.None, "no Reflection Probe on None");
		Check.True(probe.Update == ReflectionProbeUpdate.Once && !probe.BoxProjection, "nothing to read");
		probe.Update = ReflectionProbeUpdate.Live;
		probe.Interval = 0.5f;
		probe.Resolution = ReflectionProbeResolution.Size256;
		probe.CaptureOffset = new System.Numerics.Vector3(0f, -1f, 0f);
		probe.Capture();
		Check.Near(0f, probe.Interval);
		Check.True(probe.CaptureOffset == System.Numerics.Vector3.Zero, "no offset to read");
		Check.True(Obj.None.AddReflectionProbe() == ReflectionProbe.None, "nothing to add it to");
	}
}
