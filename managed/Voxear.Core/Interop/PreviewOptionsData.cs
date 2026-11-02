using System.Numerics;
using System.Runtime.InteropServices;

namespace Voxear.Interop;

/// <summary>How a preview looks, as the engine reads it. Mirrors PreviewOptions in voxear_scripting.</summary>
[StructLayout(LayoutKind.Sequential)]
internal struct PreviewOptionsData
{
	public uint Size;
	public Vector4 Background;
	public Quaternion Rotation;
	public Vector2 Offset;
	public float Zoom;
	public float FieldOfView;
	public Vector3 LightFrom;
	public Vector3 LightColor;
	public float LightIntensity;
	public float Ambient;
	public float Exposure;
}
