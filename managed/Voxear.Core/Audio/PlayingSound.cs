using System;
using Voxear.Interop;

namespace Voxear
{
	/// <summary>
	/// One play of a sound that Audio.Play started, until it ends or is stopped. Keep it to stop the sound early, or
	/// to change its volume or pitch as it plays; there is no need to keep it otherwise. Once the sound has ended,
	/// IsPlaying is false and the rest do nothing.
	/// </summary>
	public readonly struct PlayingSound : IEquatable<PlayingSound>
	{
		public static readonly PlayingSound None = default;

		private readonly ulong voice;

		internal PlayingSound(ulong voice)
		{
			this.voice = voice;
		}

		/// <summary>Whether it is playing now: started, not paused, and not yet at its end.</summary>
		public bool IsPlaying => NativeAudio.Voice(voice, NativeAudio.VoicePlaying) == 1f;

		public bool IsPaused => NativeAudio.Voice(voice, NativeAudio.VoicePaused) == 1f;

		/// <summary>From 0 for silent; 1 plays the sound as it is.</summary>
		public float Volume
		{
			get { return Value(NativeAudio.VoiceVolume, 0f); }
			set { NativeAudio.SetVoice(voice, NativeAudio.VoiceVolume, value); }
		}

		/// <summary>1 plays the sound as it is; 2 an octave higher and twice as fast; 0.5 an octave lower.</summary>
		public float Pitch
		{
			get { return Value(NativeAudio.VoicePitch, 1f); }
			set { NativeAudio.SetVoice(voice, NativeAudio.VoicePitch, value); }
		}

		/// <summary>Stops it, fading out over a few milliseconds so it does not click.</summary>
		public void Stop()
		{
			NativeAudio.SetVoice(voice, NativeAudio.VoiceStop, 1f);
		}

		/// <summary>Holds it where it is, until Resume.</summary>
		public void Pause()
		{
			NativeAudio.SetVoice(voice, NativeAudio.VoicePaused, 1f);
		}

		public void Resume()
		{
			NativeAudio.SetVoice(voice, NativeAudio.VoicePaused, 0f);
		}

		public static bool operator ==(PlayingSound left, PlayingSound right)
		{
			return left.voice == right.voice;
		}

		public static bool operator !=(PlayingSound left, PlayingSound right)
		{
			return left.voice != right.voice;
		}

		public bool Equals(PlayingSound other)
		{
			return voice == other.voice;
		}

		public override bool Equals(object? other)
		{
			return other is PlayingSound playing && Equals(playing);
		}

		public override int GetHashCode()
		{
			return voice.GetHashCode();
		}

		private float Value(int what, float ended)
		{
			var value = NativeAudio.Voice(voice, what);
			return float.IsNaN(value) ? ended : value;
		}
	}
}
