using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>Anti-aliasing, sharpening and the scale the scene is drawn at.</summary>
public sealed class AntiAliasingSettings
{
	/// <summary>Temporal, FXAA or off.</summary>
	public AntiAliasingMode Mode = AntiAliasingMode.Taa;

	/// <summary>With TAA, what builds the full picture from the frames drawn. One the GPU cannot run is Voxear's own in its place.</summary>
	public Upscaler Upscaler = Upscaler.Voxear;

	/// <summary>With TAA, how much smaller the scene is drawn, by name, in place of RenderScale; null leaves RenderScale to say.</summary>
	public UpscaleQuality? Quality;

	/// <summary>Contrast-adaptive sharpening after anti-aliasing, 0 to 1.</summary>
	public float Sharpness = 0.25f;

	/// <summary>How much of last frame TAA keeps, 0.8 to 0.97: higher is smoother and slower to react.</summary>
	public float TaaHistory = 0.97f;

	/// <summary>How far the camera is jittered within a pixel.</summary>
	public float TaaJitter = 1f;

	/// <summary>With TAA, the share of the output's width and height the scene is drawn at, from 0.25 to 1, upscaled by TAA.</summary>
	public float RenderScale = 1f;

	/// <summary>Calms the sparkle of shiny, finely bumpy surfaces, 0 to 1.</summary>
	public float SpecularSmoothing = 0.5f;

	internal void Read(JsonObject from)
	{
		PostJson.ReadChoice(from, "mode", ref Mode);
		PostJson.ReadChoice(from, "upscaler", ref Upscaler);
		PostJson.ReadChoice(from, "quality", ref Quality);
		PostJson.Read(from, "sharpness", ref Sharpness);
		PostJson.Read(from, "taa_history", ref TaaHistory);
		PostJson.Read(from, "taa_jitter", ref TaaJitter);
		PostJson.Read(from, "render_scale", ref RenderScale);
		PostJson.Read(from, "specular_smoothing", ref SpecularSmoothing);
	}

	internal void Write(JsonObject to)
	{
		to["mode"] = PostJson.Of(Mode);
		to["upscaler"] = PostJson.Of(Upscaler);
		to["quality"] = PostJson.Of(Quality);
		to["sharpness"] = Sharpness;
		to["taa_history"] = TaaHistory;
		to["taa_jitter"] = TaaJitter;
		to["render_scale"] = RenderScale;
		to["specular_smoothing"] = SpecularSmoothing;
	}
}
