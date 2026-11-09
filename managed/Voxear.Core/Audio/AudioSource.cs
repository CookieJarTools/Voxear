using System;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Audio Source, which plays a sound from it, as Unity's AudioSource does. Get it with Obj.AudioSource,
/// or give an Obj one with Obj.AddAudioSource. A 3D source sounds from where its Obj is as it moves, quieter
/// further away; a 2D one plays the same wherever the listener is, as music does.
/// <para>
/// Play, Pause and Stop take effect at the end of the frame, with every other change to the sound. IsPlaying says
/// what was asked straight away.
/// </para>
/// </summary>
public readonly struct AudioSource : IEquatable<AudioSource>, IObjComponent
{
	public static readonly AudioSource None = default;

	internal AudioSource(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj it plays from.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Audio Source, or is gone.</summary>
	public bool IsValid => Obj.IsValid && NativeAudio.Source(Obj, NativeAudio.SourceExists) == 1f;

	/// <summary>Whether it is switched on, as Unity's enabled. Off, it stops; back on, it plays again if it plays on start.</summary>
	public bool Enabled
	{
		get { return Obj.IsEnabled(EngineComponent.AudioSource); }
		set { Obj.SetEnabled(EngineComponent.AudioSource, value); }
	}

	/// <summary>Whether it is playing now, or about to at the end of the frame.</summary>
	public bool IsPlaying => NativeAudio.Source(Obj, NativeAudio.SourcePlaying) == 1f;

	/// <summary>The sound it plays. A new one takes over the next time it plays.</summary>
	public Sound Sound
	{
		get
		{
			var id = NativeAudio.Source(Obj, NativeAudio.SourceSound);
			return float.IsNaN(id) ? Sound.None : Sound.FromEngine((int)id);
		}
		set { Set(NativeAudio.SourceSound, value.EngineId); }
	}

	/// <summary>From 0 for silent; 1 plays the sound as it is.</summary>
	public float Volume
	{
		get { return Get(NativeAudio.SourceVolume, 0f); }
		set { Set(NativeAudio.SourceVolume, value); }
	}

	/// <summary>1 plays the sound as it is; 2 an octave higher and twice as fast; 0.5 an octave lower.</summary>
	public float Pitch
	{
		get { return Get(NativeAudio.SourcePitch, 1f); }
		set { Set(NativeAudio.SourcePitch, value); }
	}

	/// <summary>Whether the sound starts again from the beginning each time it ends.</summary>
	public bool Loop
	{
		get { return Get(NativeAudio.SourceLoop, 0f) == 1f; }
		set { Set(NativeAudio.SourceLoop, value ? 1f : 0f); }
	}

	/// <summary>Whether it plays by itself when its Obj comes into the running game.</summary>
	public bool PlayOnStart
	{
		get { return Get(NativeAudio.SourcePlayOnStart, 0f) == 1f; }
		set { Set(NativeAudio.SourcePlayOnStart, value ? 1f : 0f); }
	}

	/// <summary>True for a sound placed at its Obj in 3D; false for 2D. Takes effect the next time it plays.</summary>
	public bool Is3D
	{
		get { return Get(NativeAudio.SourceSpatial, 0f) == 1f; }
		set { Set(NativeAudio.SourceSpatial, value ? 1f : 0f); }
	}

	/// <summary>A 3D source is at full volume within this many metres of the listener.</summary>
	public float MinDistance
	{
		get { return Get(NativeAudio.SourceMinDistance, 0f); }
		set { Set(NativeAudio.SourceMinDistance, value); }
	}

	/// <summary>A 3D source is silent beyond this many metres from the listener.</summary>
	public float MaxDistance
	{
		get { return Get(NativeAudio.SourceMaxDistance, 0f); }
		set { Set(NativeAudio.SourceMaxDistance, value); }
	}

	/// <summary>
	/// How strongly a 3D source's pitch rises as it and the listener come together and falls as they part, as a
	/// passing car's engine drops: 1 as in the world, 0 not at all, up to 5.
	/// </summary>
	public float DopplerLevel
	{
		get { return Get(NativeAudio.SourceDoppler, 1f); }
		set { Set(NativeAudio.SourceDoppler, value); }
	}

	/// <summary>
	/// Whether walls between a 3D source and the listener stop its sound, as Steam Audio works out from the level's
	/// colliders: by one ray from its middle, or by rays across a sphere OcclusionRadius round it, so a source half
	/// behind a corner is half heard. Off by default.
	/// </summary>
	public SoundOcclusion Occlusion
	{
		get { return (SoundOcclusion)(int)Get(NativeAudio.SourceOcclusion, 0f); }
		set { Set(NativeAudio.SourceOcclusion, (int)value); }
	}

	/// <summary>The radius of the sphere a volumetric source is heard from, in metres.</summary>
	public float OcclusionRadius
	{
		get { return Get(NativeAudio.SourceOcclusionRadius, 1f); }
		set { Set(NativeAudio.SourceOcclusionRadius, value); }
	}

	/// <summary>
	/// Whether walls in the way let some of the sound through, as their Collider's AcousticMaterial does, so a source
	/// in the next room is heard quieter and duller rather than not at all. On by default.
	/// </summary>
	public bool Transmission
	{
		get { return Get(NativeAudio.SourceTransmission, 1f) == 1f; }
		set { Set(NativeAudio.SourceTransmission, value ? 1f : 0f); }
	}

	/// <summary>
	/// Whether, with Occlusion on, the sound also comes round the walls along the paths baked for the level, so a
	/// source behind a wall is heard through the doorway, from the doorway's side. Off by default, and nothing
	/// changes in a level without baked paths.
	/// </summary>
	public bool Pathing
	{
		get { return Get(NativeAudio.SourcePathing, 0f) == 1f; }
		set { Set(NativeAudio.SourcePathing, value ? 1f : 0f); }
	}

	/// <summary>Whether it plays through the effects bus or the music bus.</summary>
	public AudioBus Bus
	{
		get { return Get(NativeAudio.SourceBus, 0f) == 1f ? AudioBus.Music : AudioBus.Effects; }
		set { Set(NativeAudio.SourceBus, value == AudioBus.Music ? 1f : 0f); }
	}

	/// <summary>Plays its sound from the start, or on from where it was paused.</summary>
	public void Play()
	{
		Set(NativeAudio.SourcePlay, 1f);
	}

	/// <summary>Holds it where it is; Play carries on from there.</summary>
	public void Pause()
	{
		Set(NativeAudio.SourcePause, 1f);
	}

	/// <summary>Stops it; the next Play starts from the beginning.</summary>
	public void Stop()
	{
		Set(NativeAudio.SourceStop, 1f);
	}

	public static bool operator ==(AudioSource left, AudioSource right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(AudioSource left, AudioSource right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(AudioSource other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is AudioSource source && Equals(source);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	private float Get(int what, float missing)
	{
		var value = NativeAudio.Source(Obj, what);
		return float.IsNaN(value) ? missing : value;
	}

	private void Set(int what, float value)
	{
		if (float.IsNaN(NativeAudio.SetSource(Obj, what, value)) && Native.IsConnected)
		{
			Log.Error($"{Obj} has no Audio Source; give it one with AddAudioSource.");
		}
	}
}
