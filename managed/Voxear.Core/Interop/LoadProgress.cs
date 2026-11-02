using System.Runtime.InteropServices;

namespace Voxear.Interop;

/// <summary>A background load as the engine reports it. Mirrors LoadProgress in voxear_scripting.</summary>
[StructLayout(LayoutKind.Sequential)]
internal struct LoadProgress
{
	public const uint Loading = 0;
	public const uint Ready = 1;
	public const uint Done = 2;
	public const uint Failed = 3;

	public uint State;
	public float Progress;
	public int Prefab;
}
