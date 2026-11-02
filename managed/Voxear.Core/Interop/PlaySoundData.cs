using System.Numerics;
using System.Runtime.InteropServices;

namespace Voxear.Interop;

/// <summary>How Audio.Play plays a sound once. Mirrors PlaySound in crates/voxear_scripting/src/audio.rs.</summary>
[StructLayout(LayoutKind.Sequential)]
internal struct PlaySoundData
{
	public float Volume;
	public float Pitch;
	public int Bus;
	public int Spatial;
	public Vector3 Position;
	public float MinDistance;
	public float MaxDistance;
}
