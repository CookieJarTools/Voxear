using System.Text;

namespace Voxear.Interop;

/// <summary>
/// The engine's layers, for Obj.Layer and LayerMask. Without the engine, as in tests, every Obj is on Default and only
/// Default is named.
/// </summary>
internal static unsafe class NativeLayers
{
	public static int Of(Obj obj)
	{
		return Native.IsConnected ? System.Math.Max(Native.Api->ObjLayer(obj.Id, 0, 0), 0) : 0;
	}

	public static void Set(Obj obj, int layer)
	{
		if (Native.IsConnected)
		{
			Native.Api->ObjLayer(obj.Id, 1, layer);
		}
	}

	public static string Name(int layer)
	{
		if (!Native.IsConnected)
		{
			return layer == 0 ? "Default" : string.Empty;
		}

		var buffer = new byte[64];
		fixed (byte* written = buffer)
		{
			var length = Native.Api->LayerName(layer, written, buffer.Length);
			if (length > buffer.Length)
			{
				buffer = new byte[length];
				fixed (byte* again = buffer)
				{
					length = Native.Api->LayerName(layer, again, buffer.Length);
				}
			}
			return Encoding.UTF8.GetString(buffer, 0, System.Math.Max(length, 0));
		}
	}

	public static int Find(string name)
	{
		if (!Native.IsConnected)
		{
			return name == "Default" ? 0 : -1;
		}

		var bytes = Encoding.UTF8.GetBytes(name);
		fixed (byte* text = bytes)
		{
			return Native.Api->LayerFind(text, bytes.Length);
		}
	}
}
