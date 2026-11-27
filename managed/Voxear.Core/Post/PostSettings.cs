using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>
/// Every post-processing setting, in sections as a scene's [post] writes them. A new one is the Voxear Look. Post.Current
/// gives the settings a frame is drawn with; change them in Post.Change, or give a whole set to Post.Set.
/// <code>
/// Post.Change(post =&gt;
/// {
/// 	post.Bloom.Intensity = 0.2f;
/// 	post.Exposure.Compensation = -1f;
/// });
/// </code>
/// </summary>
public sealed class PostSettings
{
	/// <summary>Ambient occlusion: creases, corners and contacts darken, where less of the sky's light reaches. It shades only the sky's light, never the sun or lamps.</summary>
	public readonly AmbientOcclusionSettings AmbientOcclusion = new();

	/// <summary>Contact shadows: the small shadows the sun casts where things touch, which its shadow map is too coarse to hold.</summary>
	public readonly ContactShadowsSettings ContactShadows = new();

	/// <summary>Bounced light, from probes round the camera whose rays are traced against the scene.</summary>
	public readonly GlobalIlluminationSettings GlobalIllumination = new();

	/// <summary>Reflections of what is round smooth surfaces: traced where the GPU can, otherwise what is on screen. They need TAA.</summary>
	public readonly ReflectionsSettings Reflections = new();

	/// <summary>Anti-aliasing, sharpening and the scale the scene is drawn at.</summary>
	public readonly AntiAliasingSettings AntiAliasing = new();

	/// <summary>The soft glow round bright lights and highlights.</summary>
	public readonly BloomSettings Bloom = new();

	/// <summary>How bright the picture is, and how the camera adjusts to light and dark.</summary>
	public readonly ExposureSettings Exposure = new();

	/// <summary>How bright light is fitted to what a screen shows.</summary>
	public readonly TonemapSettings Tonemap = new();

	/// <summary>The colour grade over the finished picture.</summary>
	public readonly GradingSettings Grading = new();

	/// <summary>Darkened corners, drawing the eye to the middle.</summary>
	public readonly VignetteSettings Vignette = new();

	/// <summary>Height fog, and the light shafts that show in it.</summary>
	public readonly FogSettings Fog = new();

	/// <summary>Focus blur, as a camera lens's.</summary>
	public readonly DepthOfFieldSettings DepthOfField = new();

	/// <summary>Blur of what moves fast, as a film camera's.</summary>
	public readonly MotionBlurSettings MotionBlur = new();

	/// <summary>Splits colours towards the screen's edges, as a cheap lens does. 0 is off.</summary>
	public float ChromaticAberration;

	/// <summary>Fine noise over the picture, as on film or a camera in the dark.</summary>
	public readonly GrainSettings Grain = new();

	/// <summary>The view slowly breathing and wavering, as through heat or unease.</summary>
	public readonly WarpSettings Warp = new();

	/// <summary>The scene drawn at a few pixels to the screen's many and enlarged with hard edges, as pixel art is.</summary>
	public readonly PixelsSettings Pixels = new();

	/// <summary>These settings as JSON, laid out as a scene's [post] writes them, as a game's options file might keep them.</summary>
	public string ToJson()
	{
		return ToNode().ToJsonString();
	}

	/// <summary>Settings from JSON that ToJson wrote; what it leaves out is the Voxear Look's.</summary>
	public static PostSettings FromJson(string json)
	{
		var settings = new PostSettings();
		if (JsonNode.Parse(json) is JsonObject read)
		{
			settings.Read(read);
		}
		return settings;
	}

	internal void Read(JsonObject from)
	{
		AmbientOcclusion.Read(PostJson.Section(from, "ambient_occlusion"));
		ContactShadows.Read(PostJson.Section(from, "contact_shadows"));
		GlobalIllumination.Read(PostJson.Section(from, "global_illumination"));
		Reflections.Read(PostJson.Section(from, "reflections"));
		AntiAliasing.Read(PostJson.Section(from, "anti_aliasing"));
		Bloom.Read(PostJson.Section(from, "bloom"));
		Exposure.Read(PostJson.Section(from, "exposure"));
		Tonemap.Read(PostJson.Section(from, "tonemap"));
		Grading.Read(PostJson.Section(from, "grading"));
		Vignette.Read(PostJson.Section(from, "vignette"));
		Fog.Read(PostJson.Section(from, "fog"));
		DepthOfField.Read(PostJson.Section(from, "depth_of_field"));
		MotionBlur.Read(PostJson.Section(from, "motion_blur"));
		PostJson.Read(from, "chromatic_aberration", ref ChromaticAberration);
		Grain.Read(PostJson.Section(from, "grain"));
		Warp.Read(PostJson.Section(from, "warp"));
		Pixels.Read(PostJson.Section(from, "pixels"));
	}

	internal JsonObject ToNode()
	{
		var to = new JsonObject();
		to["ambient_occlusion"] = PostJson.Node(AmbientOcclusion.Write);
		to["contact_shadows"] = PostJson.Node(ContactShadows.Write);
		to["global_illumination"] = PostJson.Node(GlobalIllumination.Write);
		to["reflections"] = PostJson.Node(Reflections.Write);
		to["anti_aliasing"] = PostJson.Node(AntiAliasing.Write);
		to["bloom"] = PostJson.Node(Bloom.Write);
		to["exposure"] = PostJson.Node(Exposure.Write);
		to["tonemap"] = PostJson.Node(Tonemap.Write);
		to["grading"] = PostJson.Node(Grading.Write);
		to["vignette"] = PostJson.Node(Vignette.Write);
		to["fog"] = PostJson.Node(Fog.Write);
		to["depth_of_field"] = PostJson.Node(DepthOfField.Write);
		to["motion_blur"] = PostJson.Node(MotionBlur.Write);
		to["chromatic_aberration"] = ChromaticAberration;
		to["grain"] = PostJson.Node(Grain.Write);
		to["warp"] = PostJson.Node(Warp.Write);
		to["pixels"] = PostJson.Node(Pixels.Write);
		return to;
	}
}
