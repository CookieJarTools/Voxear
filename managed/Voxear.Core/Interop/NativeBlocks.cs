using System.Numerics;
using System.Text;

namespace Voxear.Interop;

/// <summary>Blocks read from the engine: what a .block file says, and which block a number is in a world.</summary>
internal static unsafe class NativeBlocks
{
	/// <summary>Goes up whenever a block is saved, so what was read of one is read again.</summary>
	public static ulong Version => Native.IsConnected ? Native.Api->BlocksVersion() : 0;

	/// <summary>A block by project path or GUID as JSON: its GUID, name, look and rules; null when there is none.</summary>
	public static string? Info(string reference)
	{
		if (!Native.IsConnected)
		{
			return null;
		}
		var named = Encoding.UTF8.GetBytes(reference);
		var buffer = new byte[1024];
		fixed (byte* given = named)
		{
			var length = Read(given, named.Length, buffer);
			if (length > buffer.Length)
			{
				buffer = new byte[length];
				length = Read(given, named.Length, buffer);
			}
			return length < 0 ? null : Encoding.UTF8.GetString(buffer, 0, length);
		}
	}

	/// <summary>The GUID of the block numbered <paramref name="block"/> in the world on an Obj; null for air or a kind of the world's own.</summary>
	public static string? Of(ulong obj, int block)
	{
		if (!Native.IsConnected || block <= 0)
		{
			return null;
		}
		var buffer = stackalloc byte[64];
		var length = Native.Api->BlockOf(obj, block, buffer, 64);
		return length is <= 0 or > 64 ? null : Encoding.UTF8.GetString(buffer, length);
	}

	/// <summary>Sets the blocks a shape reaches to the block <paramref name="key"/> names, in every world; how many changed.</summary>
	public static int Set(int shape, Vector3 a, Vector3 b, string key, int only)
	{
		if (!Native.IsConnected)
		{
			return 0;
		}
		var named = Encoding.UTF8.GetBytes(key);
		fixed (byte* given = named)
		{
			return Native.Api->BlockSetNamed(shape, &a, &b, given, named.Length, only);
		}
	}

	private static int Read(byte* reference, int length, byte[] buffer)
	{
		fixed (byte* answer = buffer)
		{
			return Native.Api->BlockInfo(reference, length, answer, buffer.Length);
		}
	}
}
