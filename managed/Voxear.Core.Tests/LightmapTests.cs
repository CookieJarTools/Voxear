using System.Numerics;

namespace Voxear.Tests;

/// <summary>The baked lighting API with no engine behind it: nothing is baked, no Obj has a part of it, nothing throws.</summary>
internal static class LightmapTests
{
	[Test]
	private static void NothingIsBakedWithoutTheEngine()
	{
		Check.True(!BakedLighting.IsBaked && BakedLighting.ProbeCount == 0 && !BakedLighting.IsBaking, "no bake");
		Check.True(!BakedLighting.ProbeLightAt(Vector3.One).IsValid, "no probe light");
		Check.True(Obj.None.AddLightmap() == Lightmap.None && !Obj.None.Lightmap.IsBaked, "nothing to add it to");
		var probes = Obj.None.LightProbeGroup;
		probes.AddPoint(Vector3.UnitY);
		Check.True(!probes.IsValid && probes.ProbeCount == 0, "no Light Probe Group on None");
	}

	[Test]
	private static void ProbeLightFromAboveLightsWhatFacesUp()
	{
		// Light of 1 from the whole upper half, as a bake would project it and turn it into what a surface takes.
		var harmonics = new Vector3[9];
		harmonics[0] = Vector3.One * 1.772454f;
		harmonics[1] = Vector3.One * 1.023327f;
		var light = new ProbeLight(harmonics);
		Check.Near(1f, light.Irradiance(Vector3.UnitY).X, 0.05f);
		Check.Near(0.5f, light.Irradiance(Vector3.UnitX).X, 0.05f);
		Check.Near(0f, light.Irradiance(-Vector3.UnitY).X, 0.05f);
	}
}
