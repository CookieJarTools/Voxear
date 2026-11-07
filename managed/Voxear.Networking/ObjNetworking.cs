using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>An Obj's Net Object, with the Networking module.</summary>
public static class ObjNetworking
{
	extension(Obj obj)
	{
		/// <summary>The Obj's Net Object, which shares it with a networked game's players. Not valid when it has none.</summary>
		public NetObject NetObject => new NetObject(obj);

		/// <summary>Gives the Obj a Net Object, unless it has one, and returns it: a server shares it from the next frame.</summary>
		public NetObject AddNetObject()
		{
			NativeNetObject.Set(obj, NativeNetObject.Add, Vector4.Zero);
			return new NetObject(obj);
		}
	}
}
