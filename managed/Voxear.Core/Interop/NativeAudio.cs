using System.Text;

namespace Voxear.Interop;

/// <summary>
/// Thin wrappers over the engine's sound calls. Without the engine, as in tests, nothing loads or plays, and every
/// value reads as NaN.
/// </summary>
internal static unsafe class NativeAudio
{
	public const int VoicePlaying = 0;
	public const int VoicePaused = 1;
	public const int VoiceStop = 2;
	public const int VoiceVolume = 3;
	public const int VoicePitch = 4;

	public const int SourceExists = 0;
	public const int SourceAdd = 1;
	public const int SourcePlay = 2;
	public const int SourcePause = 3;
	public const int SourceStop = 4;
	public const int SourcePlaying = 5;
	public const int SourceSound = 6;
	public const int SourceVolume = 7;
	public const int SourcePitch = 8;
	public const int SourceLoop = 9;
	public const int SourceSpatial = 10;
	public const int SourceMinDistance = 11;
	public const int SourceMaxDistance = 12;
	public const int SourceBus = 13;
	public const int SourcePlayOnStart = 14;

	public const int BusMaster = 0;
	public const int BusEffects = 1;
	public const int BusMusic = 2;

	/// <summary>The engine's id for the sound at a project path or GUID, or -1 when it will not load.</summary>
	public static int Load(string path)
	{
		if (!Native.IsConnected)
		{
			return -1;
		}

		var bytes = Encoding.UTF8.GetBytes(path);
		fixed (byte* text = bytes)
		{
			return Native.Api->SoundLoad(text, bytes.Length);
		}
	}

	public static SoundFacts Facts(int sound)
	{
		var facts = default(SoundFacts);
		if (Native.IsConnected && sound >= 0)
		{
			Native.Api->SoundInfo(sound, &facts);
		}
		return facts;
	}

	/// <summary>Plays a sound once and returns its voice, or 0 when it did not play.</summary>
	public static ulong Play(int sound, PlaySoundData play)
	{
		return Native.IsConnected && sound >= 0 ? Native.Api->SoundPlay(sound, &play) : 0;
	}

	public static float Voice(ulong voice, int what)
	{
		return Native.IsConnected ? Native.Api->VoiceValue(voice, what, 0, 0f) : float.NaN;
	}

	public static void SetVoice(ulong voice, int what, float value)
	{
		if (Native.IsConnected)
		{
			Native.Api->VoiceValue(voice, what, 1, value);
		}
	}

	public static float Source(Obj obj, int what)
	{
		return Native.IsConnected ? Native.Api->SourceValue(obj.Id, what, 0, 0f) : float.NaN;
	}

	public static float SetSource(Obj obj, int what, float value)
	{
		return Native.IsConnected ? Native.Api->SourceValue(obj.Id, what, 1, value) : float.NaN;
	}

	public static float Bus(int bus)
	{
		return Native.IsConnected ? Native.Api->AudioBus(bus, 0, 0f) : float.NaN;
	}

	public static void SetBus(int bus, float volume)
	{
		if (Native.IsConnected)
		{
			Native.Api->AudioBus(bus, 1, volume);
		}
	}
}
