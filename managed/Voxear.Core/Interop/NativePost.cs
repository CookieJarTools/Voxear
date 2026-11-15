using System.Text;

namespace Voxear.Interop;

/// <summary>Post-processing through the engine, as JSON laid out as a scene's [post] writes it.</summary>
internal static unsafe class NativePost
{
	public const int InEffect = 0;
	public const int Overrides = 1;
	public const int Set = 2;
	public const int Reset = 3;

	public static bool IsConnected => Native.IsConnected;

	/// <summary>What the engine answers; null away from it, or where it refused, as the Console then says.</summary>
	public static string? Call(int what, string text)
	{
		if (!Native.IsConnected)
		{
			return null;
		}
		var given = Encoding.UTF8.GetBytes(text);
		var buffer = new byte[8192];
		var length = Ask(what, given, buffer);
		if (length > buffer.Length)
		{
			buffer = new byte[length];
			length = Ask(what, given, buffer);
		}
		return length < 0 ? null : Encoding.UTF8.GetString(buffer, 0, length);
	}

	private static int Ask(int what, byte[] given, byte[] buffer)
	{
		fixed (byte* text = given)
		fixed (byte* answer = buffer)
		{
			return Native.Api->PostCall(what, text, given.Length, answer, buffer.Length);
		}
	}
}
