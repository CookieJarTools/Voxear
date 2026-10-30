using System.Numerics;
using System.Runtime.InteropServices;

namespace Voxear
{
	/// <summary>
	/// Position, rotation and scale of an Obj. The layout is explicit because it must match the
	/// engine's Transform byte for byte, including the padding glam adds for SIMD.
	/// </summary>
	[StructLayout(LayoutKind.Explicit, Size = 48)]
	public struct Transform
	{
		public static readonly Transform Identity = new Transform
		{
			Rotation = Quaternion.Identity,
			Scale = Vector3.One,
		};

		[FieldOffset(0)] public Vector3 Position;
		[FieldOffset(16)] public Quaternion Rotation;
		[FieldOffset(32)] public Vector3 Scale;
	}
}
