namespace Voxear;

/// <summary>
/// A mix of the game's sound to move to, as Unity's audio snapshots are: the volumes and each bus's filters. Make one
/// for each state of the game that sounds different, and slide between them with Audio.TransitionTo.
/// <code>
/// var paused = new AudioSnapshot { MusicVolume = 0.6f, MusicLowPass = 800f, EffectsVolume = 0f };
/// Audio.TransitionTo(paused, 0.5f);
/// Audio.TransitionTo(AudioSnapshot.Default, 0.5f);
/// </code>
/// </summary>
public sealed record AudioSnapshot
{
	/// <summary>A low-pass cutoff that lets everything by, in Hz.</summary>
	public const float Open = 22000f;

	/// <summary>A high-pass cutoff that lets everything by, in Hz.</summary>
	public const float Closed = 10f;

	/// <summary>Every volume full and every filter open, as the game starts.</summary>
	public static AudioSnapshot Default { get; } = new();

	public float MasterVolume { get; init; } = 1f;

	public float MusicVolume { get; init; } = 1f;

	public float EffectsVolume { get; init; } = 1f;

	public float MusicLowPass { get; init; } = Open;

	public float EffectsLowPass { get; init; } = Open;

	public float MusicHighPass { get; init; } = Closed;

	public float EffectsHighPass { get; init; } = Closed;
}
