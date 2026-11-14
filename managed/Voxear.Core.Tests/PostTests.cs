using System.Numerics;

namespace Voxear.Tests;

/// <summary>Post-processing with no engine behind it: settings to JSON and back, and changes laid over the Voxear Look.</summary>
internal static class PostTests
{
	[Test]
	private static void SettingsGoToJsonAndBack()
	{
		var settings = new PostSettings();
		settings.AmbientOcclusion.Quality = AmbientOcclusionQuality.Ultra;
		settings.AntiAliasing.Mode = AntiAliasingMode.Fxaa;
		settings.Tonemap.Mapper = Tonemapper.Aces;
		settings.Grading.Lift = new Vector3(0.1f, 0f, -0.05f);
		settings.Fog.Color = new Color(0.2f, 0.4f, 0.6f);
		settings.Grading.Lut = Texture.Load("Looks/Warm.png");
		var json = settings.ToJson();
		Check.True(json.Contains("\"quality\":\"ultra\"") && json.Contains("\"mapper\":\"aces\""), json);
		var read = PostSettings.FromJson(json);
		Check.True(read.AmbientOcclusion.Quality == AmbientOcclusionQuality.Ultra, "quality");
		Check.True(read.AntiAliasing.Mode == AntiAliasingMode.Fxaa, "mode");
		Check.Near(new Vector3(0.1f, 0f, -0.05f), read.Grading.Lift);
		Check.True(read.Fog.Color is { } fog && fog.Equals(new Color(0.2f, 0.4f, 0.6f)), "fog colour");
		Check.True(read.Grading.Lut == Texture.Load("Looks/Warm.png"), "lookup table");
		Check.True(PostSettings.FromJson("{}").Fog.Color is null, "the horizon's colour by default");
	}

	[Test]
	private static void ChangesAreOnlyWhatChanged()
	{
		Post.Reset();
		Check.True(!Post.IsChanged, "nothing changed yet");
		Check.Near(0.04f, Post.Current.Bloom.Intensity);
		Post.Change(post =>
		{
			post.Bloom.Intensity = 0.2f;
			post.AntiAliasing.RenderScale = 0.67f;
		});
		Check.True(Post.ChangesToJson() == "{\"anti_aliasing\":{\"render_scale\":0.67},\"bloom\":{\"intensity\":0.2}}", Post.ChangesToJson());
		Post.Change(post => post.Exposure.Compensation = Post.Current.Exposure.Compensation);
		Check.True(!Post.ChangesToJson().Contains("exposure"), "a setting left as it was is not changed");
		Check.Near(0.2f, Post.Current.Bloom.Intensity);
		Check.Near(0.67f, Post.Current.AntiAliasing.RenderScale);
		Post.UsePreset(PostPreset.GoldenHour);
		Check.True(Post.ChangesToJson() == "{\"look\":\"golden_hour\"}", "a look starts afresh");
		Post.UsePreset(PostPreset.Cinematic);
		Check.True(Post.ChangesToJson() == "{\"preset\":\"cinematic\"}", "a preset starts afresh");
		Post.Reset();
		Check.True(!Post.IsChanged, "reset");
	}

	[Test]
	private static void HdrIsOffAwayFromTheEngine()
	{
		Check.True(Screen.Hdr == HdrMode.Off && Screen.HdrOutput == HdrOutput.Sdr && !Screen.HdrAvailable, "SDR");
		Check.Near(200f, Screen.PaperWhite);
		Screen.Hdr = HdrMode.On;
		Check.True(Screen.HdrOutput == HdrOutput.Sdr, "nothing to show HDR on");
	}
}
