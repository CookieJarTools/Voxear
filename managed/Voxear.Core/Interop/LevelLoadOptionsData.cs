using System.Runtime.InteropServices;

namespace Voxear.Interop;

/// <summary>How a level change is made, as the engine reads it. Mirrors LevelLoadOptions in voxear_scripting.</summary>
[StructLayout(LayoutKind.Sequential)]
internal struct LevelLoadOptionsData
{
	public int Hold;
	public float LeastTime;
	public int UnloadRest;
	public int SetActive;
	public int TidyUp;
	public int ReloadShared;
}
