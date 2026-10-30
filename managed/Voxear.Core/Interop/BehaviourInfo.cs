using System.Runtime.InteropServices;

namespace Voxear.Interop
{
	/// <summary>What the engine needs to know about a behaviour type to schedule it.</summary>
	[StructLayout(LayoutKind.Sequential)]
	internal struct BehaviourInfo
	{
		public const uint InstanceFlag = 1;

		public uint Callbacks;
		public int Order;
		public uint Flags;
	}
}
