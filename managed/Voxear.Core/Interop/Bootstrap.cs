using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using Voxear.Scripting;

namespace Voxear.Interop
{
	/// <summary>
	/// The first managed code the engine runs. It keeps the engine's function table and hands back
	/// every entry point the engine needs, so nothing else has to be looked up by name.
	/// </summary>
	internal static unsafe class Bootstrap
	{
		public const uint ApiVersion = 2;

		[UnmanagedCallersOnly(CallConvs = new[] { typeof(CallConvCdecl) })]
		public static int Initialize(NativeApi* native, ManagedApi* managed)
		{
			if (native == null || managed == null || native->Version != ApiVersion)
			{
				return 1;
			}

			Native.Api = native;
			managed->Size = (uint)sizeof(ManagedApi);
			managed->Version = ApiVersion;
			managed->LoadGameAssembly = &EntryPoints.LoadGameAssembly;
			managed->CreateBehaviour = &EntryPoints.CreateBehaviour;
			managed->Invoke = &EntryPoints.Invoke;
			managed->Release = &EntryPoints.Release;
			return 0;
		}
	}
}
