using System.Runtime.InteropServices;

namespace Voxear.Interop;

/// <summary>How a scene comes in alongside those loaded, as the engine reads it. Mirrors SceneLoadOptions in voxear_scripting.</summary>
[StructLayout(LayoutKind.Sequential)]
internal struct SceneLoadOptionsData
{
	public int SetActive;
	public int Reload;
	public int Hold;
}
