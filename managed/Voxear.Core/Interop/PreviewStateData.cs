using System.Numerics;
using System.Runtime.InteropServices;

namespace Voxear.Interop;

/// <summary>Where a preview is drawn, as the engine reports it. Mirrors PreviewState in voxear_scripting.</summary>
[StructLayout(LayoutKind.Sequential)]
internal struct PreviewStateData
{
	public uint Ready;
	public uint Texture;
	public Vector4 Region;
}
