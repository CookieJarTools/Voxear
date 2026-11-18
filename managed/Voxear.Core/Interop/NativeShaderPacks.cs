using System.Text;

namespace Voxear.Interop;

/// <summary>Shader packs through the engine: which there are, which is worn and its options, and wearing one.</summary>
internal static unsafe class NativeShaderPacks
{
	/// <summary>
	/// Every shader pack there is to wear, the one worn with its options, and the player's ShaderPacks folder, as
	/// JSON; null away from the engine.
	/// </summary>
	public static string? Info()
	{
		if (!Native.IsConnected)
		{
			return null;
		}
		var buffer = new byte[4096];
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
			Native.Api->ShaderPackWear(text, bytes.Length, remember ? 1 : 0);
		}
	}

	/// <summary>Sets an option of the pack worn, by its name, after the frame.</summary>
	public static void SetOption(string name, float value, bool remember)
	{
		if (!Native.IsConnected)
		{
			return;
		}
		var bytes = Encoding.UTF8.GetBytes(name);
		fixed (byte* text = bytes)
		{
			Native.Api->ShaderPackSetOption(text, bytes.Length, value, remember ? 1 : 0);
		}
	}

	private static int Read(byte[] buffer)
	{
		fixed (byte* answer = buffer)
		{
			return Native.Api->ShaderPacksInfo(answer, buffer.Length);
		}
	}
}
