namespace Voxear.Tests;

/// <summary>The sky and the sun's look changed over the level's, with no engine behind them.</summary>
internal static class LightingTests
{
	[Test]
	private static void SkyChangesOverTheLevelsAndResets()
	{
		Lighting.ResetSky();
		Check.True(Lighting.Sky.Mode == SkyMode.Gradient && Lighting.Sky.Clouds.Cover == 0f, "a clear gradient sky");
		Lighting.ChangeSky(sky =>
		{
			sky.Mode = SkyMode.Atmosphere;
			sky.Clouds.Cover = 0.7f;
			sky.Clouds.Kind = CloudKind.Blocks;
		});
		Lighting.ChangeSky(sky => sky.Haze = 2f);
		var now = Lighting.Sky;
		Check.True(now.Mode == SkyMode.Atmosphere && now.Clouds.Kind == CloudKind.Blocks, "kept from the first change");
		Check.Near(0.7f, now.Clouds.Cover);
		Check.Near(2f, now.Haze);
		Check.Near(900f, now.Clouds.Size);
		Lighting.ResetSky();
		Check.True(Lighting.Sky.Mode == SkyMode.Gradient, "reset");
		var read = SkySettings.FromJson("{\"mode\":\"atmosphere\",\"zenith\":[0.1,0.2,0.3],\"clouds\":{\"kind\":\"blocks\",\"cover\":0.5}}");
		Check.True(read.Mode == SkyMode.Atmosphere && read.Clouds.Kind == CloudKind.Blocks, "read");
		Check.True(read.Zenith.Equals(new Color(0.1f, 0.2f, 0.3f)), "zenith");
		var written = read.ToNode().ToJsonString();
		Check.True(written.Contains("\"kind\":\"blocks\"") && written.Contains("\"distance_haze\":10"), written);
	}

	[Test]
	private static void SunLookIsTheLevelsWhereLeftNull()
	{
		Lighting.ResetSun();
		Check.True(!Lighting.SunLook.IsSet, "nothing changed");
		Lighting.ChangeSun(sun =>
		{
			sun.Temperature = 3000f;
			sun.ShadowTint = new Color(0.1f, 0.1f, 0.3f);
		});
		Lighting.ChangeSun(sun => sun.Strength = 0.5f);
		var now = Lighting.SunLook;
		Check.True(now.Temperature == 3000f && now.Strength == 0.5f && now.Tint is null, "both changes, the tint the level's");
		Check.True(now.ToNode().ToJsonString() == "{\"temperature\":3000,\"strength\":0.5,\"shadow_tint\":[0.1,0.1,0.3]}", now.ToNode().ToJsonString());
		Lighting.ChangeSun(sun => sun.Temperature = null);
		Check.True(Lighting.SunLook.Temperature is null, "back to the level's");
		Lighting.ResetSun();
		Check.True(!Lighting.SunLook.IsSet, "reset");
		var read = SunLookSettings.FromJson("{\"canopy\":12,\"tint\":[1,0.9,0.8]}");
		Check.True(read.Canopy == 12f && read.Tint is { } tint && tint.Equals(new Color(1f, 0.9f, 0.8f)), "read");
	}
}
