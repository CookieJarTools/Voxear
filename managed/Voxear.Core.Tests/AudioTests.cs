using Voxear.Scripting;

namespace Voxear.Tests;

/// <summary>The sound API with no engine behind it: nothing plays, and nothing throws.</summary>
internal static class AudioTests
{
	[Test]
	private static void SoundsAreAssetsThatFieldsAndRefsLoad()
	{
		Check.True(FieldValues.IsAsset(typeof(Sound)), "a Sound field loads with its Obj");
		Check.True(!Sound.None.IsValid && Sound.None == default, "the default is None");
		Check.True(!Sound.Load("Sounds/Door.ogg").IsValid, "nothing loads without the engine");
		var later = new Ref<Sound>("Sounds/Rain.ogg");
		Check.True(later.IsSet && later.Path == "Sounds/Rain.ogg", "a Ref names its sound");
	}

	[Test]
	private static void APlayingSoundThatNeverStartedReadsAsEnded()
	{
		var playing = Audio.Play(Sound.None);
		Check.True(playing == PlayingSound.None, "a sound that is not loaded does not play");
		Check.True(!playing.IsPlaying && !playing.IsPaused, "not playing");
		Check.Near(0f, playing.Volume);
		Check.Near(1f, playing.Pitch);
		playing.Stop();
		playing.Pause();
		playing.Resume();
	}

	[Test]
	private static void AnObjWithoutAnAudioSourceSaysSo()
	{
		var source = Obj.None.AudioSource;
		Check.True(!source.IsValid && !source.IsPlaying, "no source on None");
		Check.True(source.Sound == Sound.None, "and no sound");
		Check.Near(1f, source.Pitch);
		Check.Near(1f, source.DopplerLevel);
		Check.True(source.Occlusion == SoundOcclusion.Off && source.Transmission, "walls stop nothing, and would let some through");
		Check.Near(1f, source.OcclusionRadius);
		Check.True(!source.Pathing, "nor follows paths round them");
		Check.True(!Audio.Headphones && !Audio.HeadphonesAvailable, "no headphones without the engine");
		Check.True(!Audio.Reverb, "nor reverb");
		Audio.ReverbMode = ReverbMode.Convolution;
		Check.True(Audio.ReverbMode == ReverbMode.Parametric, "nor echoes to ring with, so parametric");
		Check.True(Audio.Ducking == null, "no ducking");
		Check.Near(AudioSnapshot.Open, Audio.GetLowPass(AudioBus.Music));
		Check.Near(AudioSnapshot.Closed, Audio.GetHighPass(AudioBus.Effects));
		Check.True(source.Bus == AudioBus.Effects && !source.Loop, "the defaults it reads as");
		source.Play();
	}

	[Test]
	private static void VolumesReadFullWithoutTheEngine()
	{
		Check.Near(1f, Audio.MasterVolume);
		Check.Near(1f, Audio.MusicVolume);
		Check.Near(1f, Audio.EffectsVolume);
		Audio.MusicVolume = 0.5f;
	}
}
