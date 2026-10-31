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
		public delegate* unmanaged[Cdecl]<byte*, int, void> LevelLoad;
		public delegate* unmanaged[Cdecl]<ulong, void> ObjDestroy;
		public delegate* unmanaged[Cdecl]<ulong, uint*, int, int> ObjBehaviours;
		public delegate* unmanaged[Cdecl]<ulong, byte*, int, int> BehaviourAdd;
		public delegate* unmanaged[Cdecl]<byte*, int, int> PrefabLoad;
		public delegate* unmanaged[Cdecl]<int, Transform*, int, ulong, ulong*, int> PrefabInstantiate;
		public delegate* unmanaged[Cdecl]<uint*, int, int> BatchHandles;
		public delegate* unmanaged[Cdecl]<uint*, uint*, int, void> BehavioursAdopt;
		public delegate* unmanaged[Cdecl]<ulong, Transform*, int> ObjWorldTransform;
		public delegate* unmanaged[Cdecl]<int, byte*, int, int> LoadAsync;
		public delegate* unmanaged[Cdecl]<uint, LoadProgress*, int> LoadStatus;
		public delegate* unmanaged[Cdecl]<uint, int, void> LoadAllowActivation;
		public delegate* unmanaged[Cdecl]<int, int, void> PrefabKeep;
		public delegate* unmanaged[Cdecl]<void> ContentUnloadUnused;
		public delegate* unmanaged[Cdecl]<ContentUsage*, int> ContentUsage;
		public delegate* unmanaged[Cdecl]<byte*, int, LevelLoadOptionsData*, int> LevelLoadAsync;
		public delegate* unmanaged[Cdecl]<int, int, byte*, int, int> LevelInfo;
		public delegate* unmanaged[Cdecl]<byte*, int, ulong*, int, int> ObjFind;
	}
}
