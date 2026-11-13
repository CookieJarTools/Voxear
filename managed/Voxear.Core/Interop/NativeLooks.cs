using System.Text;

namespace Voxear.Interop;

/// <summary>Look packs through the engine: which there are, which is worn, and wearing one.</summary>
internal static unsafe class NativeLooks
{
	/// <summary>
	/// Every look pack there is to wear, the one worn and the player's Looks folder, as JSON; null away from the
	/// engine.
	/// </summary>
	public static string? Info()
	{
		if (!Native.IsConnected)
		{
			return null;
		}
		var buffer = new byte[2048];
		var length = Read(buffer);
		if (length > buffer.Length)
		{
			buffer = new byte[length];
			length = Read(buffer);
		}
		return length < 0 ? null : Encoding.UTF8.GetString(buffer, 0, length);
	}

	/// <summary>Wears the pack a reference names from the next frame, or with an empty one, none.</summary>
	public static void Wear(string reference, bool remember)
	{
		if (!Native.IsConnected)
		{
			return;
		}
		var bytes = Encoding.UTF8.GetBytes(reference);
		fixed (byte* text = bytes)
		{
			Native.Api->LookPackWear(text, bytes.Length, remember ? 1 : 0);
		}
	}

	private static int Read(byte[] buffer)
	{
		fixed (byte* answer = buffer)
		{
			return Native.Api->LookPacksInfo(answer, buffer.Length);
		}
	}
}
