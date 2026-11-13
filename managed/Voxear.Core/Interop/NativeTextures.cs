using System.Text;

namespace Voxear.Interop;

/// <summary>Texture packs through the engine: which there are, which is worn, wearing one and starting one.</summary>
internal static unsafe class NativeTextures
{
	/// <summary>
	/// Every texture pack there is to wear, the one worn and the player's Textures folder, as JSON; null away from
	/// the engine.
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
			Native.Api->TexturePackWear(text, bytes.Length, remember ? 1 : 0);
		}
	}

	/// <summary>
	/// Makes a pack to start from in the player's Textures folder, and gives what names it; null where it could not
	/// be made.
	/// </summary>
	public static string? Starter(string name)
	{
		if (!Native.IsConnected)
		{
			return null;
		}
		var bytes = Encoding.UTF8.GetBytes(name);
		var buffer = new byte[1024];
		int length;
		fixed (byte* text = bytes)
		fixed (byte* answer = buffer)
		{
			length = Native.Api->TexturePackStarter(text, bytes.Length, answer, buffer.Length);
		}
		return length <= 0 || length > buffer.Length ? null : Encoding.UTF8.GetString(buffer, 0, length);
	}

	private static int Read(byte[] buffer)
	{
		fixed (byte* answer = buffer)
		{
			return Native.Api->TexturePacksInfo(answer, buffer.Length);
		}
	}
}
