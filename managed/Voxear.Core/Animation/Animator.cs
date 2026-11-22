using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Animator, which plays one clip on its model. Get it with Obj.Animator, null when the Obj has none, or
/// give an Obj one with Obj.AddAnimator. A change of clip eases from the old pose into the new over BlendTime.
/// <code>
/// // A guard who stops to look round, then walks on.
/// Obj.Animator?.Play(new AnimationClip("Models/Guard.glb#clip:LookAround"));
/// </code>
/// </summary>
public sealed class Animator : WholeComponent
{
	internal Animator(Obj obj)
		: base(obj, NativeWhole.Whole, NativeWhole.Animator)
	{
	}

	/// <summary>The clip it plays; no clip holds the model at rest.</summary>
	public AnimationClip Clip
	{
		get => new(Settings()["clip"]?.GetValue<string>() ?? string.Empty);
		set => Change("clip", value.Reference ?? string.Empty);
	}

	/// <summary>How fast it plays: 1 as the clip was made, 2 twice as fast.</summary>
	public float Speed
	{
		get => Number("speed", 1f);
		set => Change("speed", value);
	}

	/// <summary>Going round from the end to the start, or holding the last frame.</summary>
	public bool Looping
	{
		get => Switch("looping", true);
		set => Change("looping", value);
	}

	/// <summary>How long, in seconds, a change of clip takes to settle into the new one.</summary>
	public float BlendTime
	{
		get => Number("blend", 0.2f);
		set => Change("blend", value);
	}

	/// <summary>Moving the Obj along the ground as the clip's root bone travels, rather than the model away from it.</summary>
	public bool RootMotion
	{
		get => Switch("root_motion", false);
		set => Change("root_motion", value);
	}

	/// <summary>Plays <paramref name="clip"/>, looping or not, easing into it from what played before.</summary>
	public void Play(AnimationClip clip, bool looping = true)
	{
		Looping = looping;
		Clip = clip;
	}

	/// <summary>Holds the model at rest.</summary>
	public void Stop()
	{
		Clip = default;
	}

	internal static Animator? Of(Obj obj)
	{
		var animator = new Animator(obj);
		return animator.IsValid ? animator : null;
	}

	public override string ToString()
	{
		return $"{Obj}'s Animator";
	}
}
