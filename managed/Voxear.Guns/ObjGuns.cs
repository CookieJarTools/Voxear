using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>An Obj's Gun and Gun Holder, with the Guns module.</summary>
public static class ObjGuns
{
	extension(Obj obj)
	{
		/// <summary>The Obj's Gun. Not valid when it has none.</summary>
		public Gun Gun => new Gun(obj);

		/// <summary>Gives the Obj a Gun, unless it has one, and returns it.</summary>
		public Gun AddGun()
		{
			NativeComponent.Set(NativeComponent.Gun, obj, NativeComponent.Add, Vector4.Zero);
			return new Gun(obj);
		}

		/// <summary>The Obj's Gun Holder. Not valid when it has none.</summary>
		public GunHolder GunHolder => new GunHolder(obj);

		/// <summary>Gives the Obj a Gun Holder, unless it has one, and returns it.</summary>
		public GunHolder AddGunHolder()
		{
			NativeComponent.Set(NativeComponent.GunHolder, obj, NativeComponent.Add, Vector4.Zero);
			return new GunHolder(obj);
		}
	}
}
