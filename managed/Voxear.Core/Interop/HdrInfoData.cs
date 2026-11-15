using System.Runtime.InteropServices;

namespace Voxear.Interop;

/// <summary>The display's HDR output as the engine reads and sets it.</summary>
[StructLayout(LayoutKind.Sequential)]
internal struct HdrInfoData
{
	public int Mode;
	public int Output;
	public int Offered;
	public float PaperWhite;
	public float Peak;
}
