using System.Runtime.InteropServices;

namespace Voxear.Interop;

/// <summary>What the engine needs to know about a behaviour type to schedule it.</summary>
[StructLayout(LayoutKind.Sequential)]
internal struct BehaviourInfo
{
	public const uint InstanceFlag = 1;

	/// <summary>After a reload: the behaviour's class is not in the new build, so it was dropped.</summary>
	public const uint GoneFlag = 2;

	public uint Callbacks;
	public int Order;
	public uint Flags;
}
