using System;
using System.Text.Json.Nodes;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Post Volume: a box or sphere, or the whole level, whose post-processing blends in as the camera comes into
/// it, over the level's and what Post changed. Get it with Obj.PostVolume, null when the Obj has none, or give an Obj
/// one with Obj.AddPostVolume.
/// <code>
/// // A cave that darkens as the player goes in.
/// var cave = Obj.AddPostVolume();
/// cave.BlendDistance = 6f;
/// cave.ChangePost(post =&gt;
/// {
///     post.Exposure.Compensation = -1f;
///     post.Fog.Density = 0.02f;
/// });
/// </code>
/// </summary>
public sealed class PostVolume : WholeComponent
{
	internal PostVolume(Obj obj)
		: base(obj, NativeWhole.PostVolume)
	{
	}

	/// <summary>Where its settings apply.</summary>
	public PostVolumeShape Shape
	{
		get => Choice("shape", PostVolumeShape.Box);
		set => Change("shape", PostJson.Of(value));
	}

	/// <summary>How far outside its shape, in metres, its settings start to blend in; 0 switches them on at its edge.</summary>
	public float BlendDistance
	{
		get => Number("blend_distance", 1f);
		set => Change("blend_distance", value);
	}

	/// <summary>How much of its settings show at most, from 0 to 1.</summary>
	public float Weight
	{
		get => Number("weight", 1f);
		set => Change("weight", value);
	}

	/// <summary>Volumes apply in order of priority, the highest last, so it wins.</summary>
	public int Priority
	{
		get => (int)Number("priority", 0f);
		set => Change("priority", value);
	}

	/// <summary>
	/// Its settings over the Voxear Look: what it changes, and the look's in all else. A copy: changing it changes
	/// nothing until it goes to ChangePost.
	/// </summary>
	public PostSettings Post
	{
		get
		{
			var settings = new PostSettings();
			settings.Read(Own());
			return settings;
		}
	}

	/// <summary>The settings it changes, as JSON laid out as a scene's [post] writes them.</summary>
	public string PostToJson()
	{
		return Own().ToJsonString();
	}

	/// <summary>Changes the settings it changes: each one the change sets to other than it is now, the rest as they were.</summary>
	public void ChangePost(Action<PostSettings> change)
	{
		var own = Own();
		var settings = new PostSettings();
		settings.Read(own);
		var before = settings.ToNode();
		change(settings);
		PostJson.Changed(before, settings.ToNode(), own);
		Change("post", own);
	}

	/// <summary>Takes off every setting it changes, so it changes nothing.</summary>
	public void ClearPost()
	{
		Change("post", new JsonObject());
	}

	/// <summary>The Obj's Post Volume, or null when it has none.</summary>
	internal static PostVolume? Of(Obj obj)
	{
		var volume = new PostVolume(obj);
		return volume.IsValid ? volume : null;
	}

	public override string ToString()
	{
		return $"{Obj}'s Post Volume";
	}

	private JsonObject Own()
	{
		return Settings()["post"]?.DeepClone() as JsonObject ?? [];
	}
}
