using System;
using System.Text.Json.Nodes;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The post-processing a frame is drawn with, read and changed while the game runs, as a settings menu or a moment of
/// the game does. What scripts change is laid over the level's own settings, a setting at a time, and stays through level
/// changes until Reset; Post Volumes still blend in over it where the camera is.
/// <code>
/// // The easy way: a preset, or a look's post-processing, to start from.
/// Post.UsePreset(PostPreset.Cinematic);
///
/// // The full way: any setting, by its section.
/// Post.Change(post =&gt;
/// {
/// 	post.AntiAliasing.RenderScale = 0.67f;
/// 	post.AmbientOcclusion.Quality = AmbientOcclusionQuality.High;
/// 	post.Bloom.Intensity = 0.1f;
/// });
/// </code>
/// </summary>
public static class Post
{
	private static JsonObject overrides = [];

	/// <summary>
	/// The settings a frame is drawn with now, but for Post Volumes, which depend on where the camera is: the level's,
	/// a look pack's where one is worn, and what scripts changed over them. A copy: changing it changes nothing until it
	/// goes to Set.
	/// </summary>
	public static PostSettings Current
	{
		get
		{
			if (NativePost.Call(NativePost.InEffect, string.Empty) is { } json)
			{
				return PostSettings.FromJson(json);
			}
			// Away from the engine, the Voxear Look with what scripts changed.
			var settings = new PostSettings();
			settings.Read(overrides);
			return settings;
		}
	}

	/// <summary>Whether scripts have changed any setting over the level's.</summary>
	public static bool IsChanged => Overrides().Count > 0;

	/// <summary>
	/// Changes settings over the level's: each one the change sets to other than it is now, the rest left to the level.
	/// A setting given a choice it does not have is reported in the Console, and nothing changes.
	/// </summary>
	public static void Change(Action<PostSettings> change)
	{
		var settings = Current;
		var before = settings.ToNode();
		change(settings);
		var into = Overrides();
		PostJson.Changed(before, settings.ToNode(), into);
		Send(into);
	}

	/// <summary>Sets every setting as given, over the level's, until Reset.</summary>
	public static void Set(PostSettings settings)
	{
		Send(settings.ToNode());
	}

	/// <summary>
	/// Starts from a preset, or a lighting look's post-processing, in place of the level's settings and of what scripts
	/// changed before. Change then changes settings over it.
	/// </summary>
	public static void UsePreset(PostPreset preset)
	{
		var key = PostJson.Key(preset.ToString());
		var start = preset is PostPreset.Voxear or PostPreset.Neutral or PostPreset.Cinematic ? "preset" : "look";
		Send(new JsonObject { [start] = key });
	}

	/// <summary>Takes off everything scripts changed: the level's own settings show again.</summary>
	public static void Reset()
	{
		overrides = [];
		NativePost.Call(NativePost.Reset, string.Empty);
	}

	/// <summary>What scripts changed, as JSON laid out as a scene's [post] writes it.</summary>
	public static string ChangesToJson()
	{
		return Overrides().ToJsonString();
	}

	private static JsonObject Overrides()
	{
		if (NativePost.Call(NativePost.Overrides, string.Empty) is { } json && JsonNode.Parse(json) is JsonObject read)
		{
			return read;
		}
		return (JsonObject)overrides.DeepClone();
	}

	private static void Send(JsonObject next)
	{
		if (!NativePost.IsConnected)
		{
			overrides = next;
			return;
		}
		NativePost.Call(NativePost.Set, next.ToJsonString());
	}
}
