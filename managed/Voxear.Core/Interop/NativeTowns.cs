using System.Numerics;
using System.Text;

namespace Voxear.Interop;

/// <summary>Towns read from the engine, as JSON.</summary>
internal static unsafe class NativeTowns
{
	/// <summary>
	/// The towns of the world of blocks on <paramref name="obj"/> (0 for every one) whose sites lie within
	/// <c>at.W</c> metres of <c>at</c>; null where none can be read.
	/// </summary>
	public static string? Read(ulong obj, Vector4 at)
	{
		if (!Native.IsConnected)
		{
			return null;
		}
		var buffer = new byte[16384];
		var length = Ask(obj, at, buffer);
		if (length > buffer.Length)
		{
			buffer = new byte[length];
			length = Ask(obj, at, buffer);
		}
		return length < 0 ? null : Encoding.UTF8.GetString(buffer, 0, length);
	}

	private static int Ask(ulong obj, Vector4 at, byte[] buffer)
	{
		fixed (byte* answer = buffer)
		{
			return Native.Api->TownsInfo(obj, &at, answer, buffer.Length);
		}
	}
}
