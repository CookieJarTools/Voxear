using System.Runtime.InteropServices;

namespace Voxear.Interop;

/// <summary>What a loaded sound holds. Mirrors SoundFacts in crates/voxear_scripting/src/audio.rs.</summary>
[StructLayout(LayoutKind.Sequential)]
internal struct SoundFacts
{
	public float Seconds;
	public int SampleRate;
	public int Channels;
	public int Streamed;
}
