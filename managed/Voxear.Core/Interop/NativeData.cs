using System.Text;

namespace Voxear.Interop;

/// <summary>Data assets read from the engine, for Definitions.</summary>
internal static unsafe class NativeData
{
	/// <summary>A data asset by project path or GUID as JSON: its GUID, path, type and fields; null when there is none.</summary>
	public static string? Fields(string reference)
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

	private static int Read(byte* reference, int length, byte[] buffer)
	{
		fixed (byte* answer = buffer)
		{
			return Native.Api->AssetFields(reference, length, answer, buffer.Length);
		}
	}
}
