using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The game's sound. Audio.Play plays a sound once: in 2D, the same wherever the listener is, as interface sounds
/// are, or at a place in the world, where it is quieter further away and comes from its side. The listener is the
/// first Obj with an Audio Listener, or else the camera. For a sound that belongs to an Obj, and moves with it or
/// loops, give the Obj an Audio Source instead.
/// <para>
/// The volumes go from 0 for silent to 1 for full, as a settings menu sets them: MasterVolume over everything, and
/// one each for music and effects.
/// </para>
/// </summary>
public static class Audio
{
	/// <summary>The volume over everything the game plays.</summary>
	public static float MasterVolume
	{
		get { return Volume(NativeAudio.BusMaster); }
		set { NativeAudio.SetBus(NativeAudio.BusMaster, value); }
	}

	/// <summary>The volume of what plays through the music bus.</summary>
	public static float MusicVolume
	{
		get { return Volume(NativeAudio.BusMusic); }
		set { NativeAudio.SetBus(NativeAudio.BusMusic, value); }
	}

	/// <summary>The volume of what plays through the effects bus, which sounds use unless told otherwise.</summary>
	public static float EffectsVolume
	{
		get { return Volume(NativeAudio.BusEffects); }
		set { NativeAudio.SetBus(NativeAudio.BusEffects, value); }
	}

	/// <summary>
	/// Plays a sound once in 2D, the same wherever the listener is: for interface sounds, stingers and voices in
	/// the player's head. Pitch 1 plays it as it is; 2 an octave higher and twice as fast.
	/// </summary>
	public static PlayingSound Play(Sound sound, float volume = 1f, float pitch = 1f, AudioBus bus = AudioBus.Effects)
	{
		var play = new PlaySoundData { Volume = volume, Pitch = pitch, Bus = (int)bus };
		return Start(sound, play);
	}

	/// <summary>
	/// Plays a sound once at a place in the world. It is at full volume within minDistance metres of the listener,
	/// fades the way sound does in the open further out, and is silent beyond maxDistance.
	/// </summary>
	public static PlayingSound Play(Sound sound, Vector3 at, float volume = 1f, float pitch = 1f, float minDistance = 1f, float maxDistance = 50f, AudioBus bus = AudioBus.Effects)
	{
		var play = new PlaySoundData
		{
			Volume = volume,
			Pitch = pitch,
			Bus = (int)bus,
			Spatial = 1,
			Position = at,
			MinDistance = minDistance,
			MaxDistance = maxDistance,
		};
		return Start(sound, play);
	}

	private static PlayingSound Start(Sound sound, PlaySoundData play)
	{
		if (!sound.IsValid)
		{
			Log.Error("Audio.Play was given a Sound that is not loaded.");
			return PlayingSound.None;
		}
		return new PlayingSound(NativeAudio.Play(sound.EngineId, play));
	}

	private static float Volume(int bus)
	{
		var volume = NativeAudio.Bus(bus);
		return float.IsNaN(volume) ? 1f : volume;
	}
}
