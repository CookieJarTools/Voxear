using System.Numerics;
using System.Text;

namespace Voxear.Interop;

/// <summary>Cave rooms read from the engine, as JSON.</summary>
internal static unsafe class NativeCaves
{
	public const int Level = 0;
	public const int Near = 1;
	public const int Inside = 2;
	public const int Region = 3;
	public const int None = 4;

	/// <summary>
	/// The rooms <paramref name="what"/> asks for, of the world of blocks on <paramref name="obj"/> or, for 0, of
	/// the first with caves; null where there are none.
	/// </summary>
	public static string? Rooms(ulong obj, int what, Vector4 at)
	{
		if (!Native.IsConnected)
		{
			return null;
		}
		var buffer = new byte[4096];
		var length = Read(obj, what, at, buffer);
		if (length > buffer.Length)
		{
			buffer = new byte[length];
			length = Read(obj, what, at, buffer);
		}
		return length < 0 ? null : Encoding.UTF8.GetString(buffer, 0, length);
	}

	private static int Read(ulong obj, int what, Vector4 at, byte[] buffer)
	{
		fixed (byte* answer = buffer)
		{
			return Native.Api->CavesInfo(obj, what, &at, answer, buffer.Length);
		}
	}
}
