using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text;

namespace Voxear.Interop;

/// <summary>
/// The engine's text, for Localization. Without the engine, as in tests, a key is its own text and the game is in
/// English.
/// </summary>
internal static unsafe class NativeText
{
	public static string Get(string key, (string Name, object? Value)[]? values)
	{
		if (!Native.IsConnected)
		{
			return key;
		}

		var keyBytes = Encoding.UTF8.GetBytes(key);
		var valueBytes = Pack(values);
		var buffer = new byte[256];
		fixed (byte* named = keyBytes, filled = valueBytes)
		{
			var length = Get(named, keyBytes.Length, filled, valueBytes.Length, buffer);
			if (length > buffer.Length)
			{
				buffer = new byte[length];
				length = Get(named, keyBytes.Length, filled, valueBytes.Length, buffer);
			}
			return Encoding.UTF8.GetString(buffer, 0, Math.Max(length, 0));
		}
	}

	/// <summary>The language the game is in, after it is set to the nearest to code when code is given.</summary>
	public static string Language(string? code)
	{
		if (!Native.IsConnected)
		{
			return "en";
		}

		var codeBytes = Encoding.UTF8.GetBytes(code ?? string.Empty);
		var buffer = new byte[32];
		fixed (byte* wanted = codeBytes, written = buffer)
		{
			var length = Native.Api->TextLanguage(code is null ? 0 : 1, wanted, codeBytes.Length, written, buffer.Length);
			return Encoding.UTF8.GetString(buffer, 0, Math.Clamp(length, 0, buffer.Length));
		}
	}

	public static IReadOnlyList<string> Languages()
	{
		if (!Native.IsConnected)
		{
			return ["en"];
		}

		var buffer = new byte[256];
		var length = Languages(buffer);
		if (length > buffer.Length)
		{
			buffer = new byte[length];
			length = Languages(buffer);
		}
		return Encoding.UTF8.GetString(buffer, 0, Math.Max(length, 0)).Split('\n');
	}

	/// <summary>Counts each change of the game's language or text.</summary>
	public static ulong Version()
	{
		return Native.IsConnected ? Native.Api->TextVersion() : 0;
	}

	private static int Get(byte* key, int keyLength, byte* values, int valuesLength, byte[] buffer)
	{
		fixed (byte* written = buffer)
		{
			return Native.Api->TextGet(key, keyLength, values, valuesLength, written, buffer.Length);
		}
	}

	private static int Languages(byte[] buffer)
	{
		fixed (byte* written = buffer)
		{
			return Native.Api->TextLanguages(written, buffer.Length);
		}
	}

	/// <summary>Each name and its value, one after the other, each ended by a zero byte.</summary>
	private static byte[] Pack((string Name, object? Value)[]? values)
	{
		if (values is null || values.Length == 0)
		{
			return [];
		}

		var packed = new StringBuilder();
		foreach (var (name, value) in values)
		{
			packed.Append(name).Append('\0');
			packed.Append(Convert.ToString(value, CultureInfo.InvariantCulture)).Append('\0');
		}
		return Encoding.UTF8.GetBytes(packed.ToString());
	}
}
