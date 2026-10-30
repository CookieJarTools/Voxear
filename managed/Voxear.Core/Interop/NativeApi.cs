using System.Runtime.InteropServices;

namespace Voxear.Interop
{
	/// <summary>
	/// Engine functions that scripts call. The layout mirrors NativeApi in
	/// crates/voxear_scripting/src/interop.rs, and the two must change together.
	/// </summary>
	[StructLayout(LayoutKind.Sequential)]
	internal unsafe struct NativeApi
	{
		public uint Size;
		public uint Version;
		public TimeData* Time;
		public delegate* unmanaged[Cdecl]<int, byte*, int, void> Log;
		public delegate* unmanaged[Cdecl]<ulong, byte*, int, int> ObjName;
		public delegate* unmanaged[Cdecl]<ulong, Transform*, int> TransformGet;
		public delegate* unmanaged[Cdecl]<ulong, Transform*, int> TransformSet;
		public delegate* unmanaged[Cdecl]<uint, int, void> BehaviourSetEnabled;
		public delegate* unmanaged[Cdecl]<uint, void> BehaviourDestroy;
	}
}
