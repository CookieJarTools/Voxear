using System.Runtime.InteropServices;

namespace Voxear.Interop;

/// <summary>
/// Script functions that the engine calls, filled in by Bootstrap.Initialize. The layout mirrors
/// ManagedApi in crates/voxear_scripting/src/interop.rs.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal unsafe struct ManagedApi
{
	public uint Size;
	public uint Version;
	public delegate* unmanaged[Cdecl]<byte*, int, int> LoadGameAssembly;
	public delegate* unmanaged[Cdecl]<byte*, int, ulong, byte*, int, int, BehaviourInfo*, int> CreateBehaviour;
	public delegate* unmanaged[Cdecl]<int, uint*, int, void> Invoke;
	public delegate* unmanaged[Cdecl]<uint*, int, void> Release;
	public delegate* unmanaged[Cdecl]<void> Update;
	public delegate* unmanaged[Cdecl]<byte*, int, int> ContentRoots;
	public delegate* unmanaged[Cdecl]<byte*, int, void> SwitchLevel;
	public delegate* unmanaged[Cdecl]<byte*, int, byte*, int, int> RunCommand;
	public delegate* unmanaged[Cdecl]<byte*, int, uint*, uint*, BehaviourInfo*, int, int> ReloadGameAssembly;
	public delegate* unmanaged[Cdecl]<int, void> RunSystems;
	public delegate* unmanaged[Cdecl]<byte*, int, ulong, byte*, int, int> AddComponent;
	public delegate* unmanaged[Cdecl]<byte*, int, int> ConsoleRun;
	public delegate* unmanaged[Cdecl]<byte*, int, int> ConsoleNames;
	public delegate* unmanaged[Cdecl]<byte*, int, byte*, int, byte*, int, int*, int> RunCheck;
	public delegate* unmanaged[Cdecl]<byte*, int, byte*, int, int> InspectorCall;
	public delegate* unmanaged[Cdecl]<ulong*, int, int> ManagedMemory;
	public delegate* unmanaged[Cdecl]<PhysicsEventData*, int, void> PhysicsEvents;
	public delegate* unmanaged[Cdecl]<ulong*, int, byte*, int, void> MotionMarks;
	public delegate* unmanaged[Cdecl]<void> TakeNetFields;
	public delegate* unmanaged[Cdecl]<uint, int, void> SetEnabled;
	public delegate* unmanaged[Cdecl]<int*, void> OriginMoved;
}
