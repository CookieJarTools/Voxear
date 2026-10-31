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
		public delegate* unmanaged[Cdecl]<int, byte*, int, byte*, int, ulong, void> Log;
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
		public delegate* unmanaged[Cdecl]<byte*, int, uint> ProfilerMarker;
		public delegate* unmanaged[Cdecl]<uint, int> ProfilerBegin;
		public delegate* unmanaged[Cdecl]<void> ProfilerEnd;
		public delegate* unmanaged[Cdecl]<int> ProfilerEnabled;
		public delegate* unmanaged[Cdecl]<int, byte*, int, PreviewOptionsData*, int> PreviewCreate;
		public delegate* unmanaged[Cdecl]<int, PreviewStateData*, int> PreviewState;
		public delegate* unmanaged[Cdecl]<int, byte*, int, int> PreviewSave;
		public delegate* unmanaged[Cdecl]<int, void> PreviewFree;
		public ulong* WorldId;
		public delegate* unmanaged[Cdecl]<byte*, int, int, int, int> ComponentRegister;
		public delegate* unmanaged[Cdecl]<QueryDesc*, nint*, int, int> QueryChunks;
		public delegate* unmanaged[Cdecl]<ulong, uint, void> ChunkWritten;
		public delegate* unmanaged[Cdecl]<ulong> EntityCreate;
		public delegate* unmanaged[Cdecl]<ulong, void> EntityDestroy;
		public delegate* unmanaged[Cdecl]<ulong, int> EntityAlive;
		public delegate* unmanaged[Cdecl]<ulong, uint, int> ComponentHas;
		public delegate* unmanaged[Cdecl]<ulong, uint, byte*, int, int> ComponentGet;
		public delegate* unmanaged[Cdecl]<ulong, uint, byte*, int, int> ComponentSet;
		public delegate* unmanaged[Cdecl]<ulong, uint, byte*, int, int> ComponentAdd;
		public delegate* unmanaged[Cdecl]<ulong, uint, int> ComponentRemove;
		public delegate* unmanaged[Cdecl]<int, void> CommandsDefer;
		public delegate* unmanaged[Cdecl]<int, uint> ChangeTick;
	}
}
