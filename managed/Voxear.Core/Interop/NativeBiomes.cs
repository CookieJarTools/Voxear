using System.Numerics;
using System.Text;

namespace Voxear.Interop;

/// <summary>Biomes read from the engine: what a .biome file says, and which biome a place is.</summary>
internal static unsafe class NativeBiomes
{
	/// <summary>A biome by project path or GUID as JSON: its GUID, name and colour; null when there is none.</summary>
	public static string? Info(string reference)
	{
		if (!Native.IsConnected)
		{
			return null;
		}
		var named = Encoding.UTF8.GetBytes(reference);
		var buffer = new byte[256];
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

	/// <summary>The GUID of the biome the place at <paramref name="position"/> is; null where no world with biomes reaches.</summary>
	public static string? At(Vector3 position)
	{
		if (!Native.IsConnected)
		{
			return null;
		}
		var buffer = stackalloc byte[64];
		var length = Native.Api->BiomeAt(&position, buffer, 64);
		return length is <= 0 or > 64 ? null : Encoding.UTF8.GetString(buffer, length);
	}

	private static int Read(byte* reference, int length, byte[] buffer)
	{
		fixed (byte* answer = buffer)
		{
			return Native.Api->BiomeInfo(reference, length, answer, buffer.Length);
		}
	}
}
