using System;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A sound file (WAV, OGG Vorbis, FLAC or MP3), loaded and ready to play. A Sound field is loaded with its Obj,
/// before Awake, as a Prefab field is; a Ref&lt;Sound&gt; field loads it only when asked. Play one once with
/// Audio.Play, or from an Obj with its Audio Source. A short sound is decoded whole when it loads, so it starts at
/// once; a long one streams as it plays, as the sound's import settings say.
/// </summary>
public readonly struct Sound : IEquatable<Sound>, IAsset<Sound>
{
	public static readonly Sound None = default;

	/// <summary>The engine's id plus one, so that the default value is None.</summary>
	private readonly int id;

	private Sound(int engineId)
	{
		id = engineId + 1;
	}

	public bool IsValid => id > 0;

	/// <summary>The engine's id for the sound, or -1 for None.</summary>
	internal int EngineId => id - 1;

	/// <summary>How long it plays at its own pitch, in seconds.</summary>
	public float Length => NativeAudio.Facts(EngineId).Seconds;

	/// <summary>One for mono, two for stereo. A file with more plays them mixed down to stereo.</summary>
	public int Channels => NativeAudio.Facts(EngineId).Channels;

	/// <summary>Samples a second in each channel, as the file holds them, such as 44 100.</summary>
	public int SampleRate => NativeAudio.Facts(EngineId).SampleRate;

	/// <summary>Whether it streams from its file as it plays, rather than sitting decoded in memory.</summary>
	public bool IsStreamed => NativeAudio.Facts(EngineId).Streamed != 0;

	string? IAsset<Sound>.AssetGuid => IsValid ? NativeSaves.Key(NativeSaves.KeySound, (ulong)EngineId, string.Empty) : null;

	internal static Sound FromEngine(int engineId)
	{
		return engineId < 0 ? None : new Sound(engineId);
	}

	/// <summary>
	/// Loads a sound by project path or GUID, for example "Sounds/Door.ogg". Loading one that is already loaded
	/// costs nothing. Logs an error and returns None when it cannot be loaded.
	/// </summary>
	public static Sound Load(string path)
	{
		return FromEngine(NativeAudio.Load(path));
	}

	/// <summary>Plays the sound once, in 2D, as Audio.Play does.</summary>
	public PlayingSound Play(float volume = 1f)
	{
		return Audio.Play(this, volume);
	}

	public static bool operator ==(Sound left, Sound right)
	{
		return left.id == right.id;
	}

	public static bool operator !=(Sound left, Sound right)
	{
		return left.id != right.id;
	}

	public bool Equals(Sound other)
	{
		return id == other.id;
	}

	public override bool Equals(object? other)
	{
		return other is Sound sound && Equals(sound);
	}

	public override int GetHashCode()
	{
		return id;
	}
}
