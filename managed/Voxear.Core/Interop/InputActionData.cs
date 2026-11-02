using System.Runtime.InteropServices;

namespace Voxear.Interop;

/// <summary>An action as it is this frame. Mirrors InputActionData in crates/voxear_scripting/src/input.rs.</summary>
[StructLayout(LayoutKind.Sequential)]
internal struct InputActionData
{
	public const int Held = 1;
	public const int Pressed = 2;
	public const int Released = 4;
	public const int Performed = 8;

	public float X;
	public float Y;
	public int Flags;
	public float HeldFor;
}
