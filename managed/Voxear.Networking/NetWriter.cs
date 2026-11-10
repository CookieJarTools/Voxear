using System;
using System.Collections.Generic;
using System.Numerics;
using System.Text;
using Voxear.Interop;

namespace Voxear;

/// <summary>Writes replicated values as bytes, little end first, for the network.</summary>
public sealed class NetWriter
{
	private readonly List<byte> bytes = new List<byte>();

	public void Write(bool value)
	{
		bytes.Add(value ? (byte)1 : (byte)0);
	}

	public void Write(int value)
	{
		Span<byte> data = stackalloc byte[4];
		BitConverter.TryWriteBytes(data, value);
		Add(data);
	}

	public void Write(long value)
	{
		Span<byte> data = stackalloc byte[8];
		BitConverter.TryWriteBytes(data, value);
		Add(data);
	}

	public void Write(float value)
	{
		Write(BitConverter.SingleToInt32Bits(value));
	}

	public void Write(double value)
	{
		Write(BitConverter.DoubleToInt64Bits(value));
	}

	public void Write(string? value)
	{
		var text = Encoding.UTF8.GetBytes(value ?? string.Empty);
		Write(text.Length);
		bytes.AddRange(text);
	}

	/// <summary>Bytes as they are, after their count: a terrain's edits for a player who joins, say.</summary>
	public void Write(byte[]? value)
	{
		Write(value?.Length ?? 0);
		bytes.AddRange(value ?? []);
	}

	public void Write(Vector2 value)
	{
		Write(value.X);
		Write(value.Y);
	}

	public void Write(Vector3 value)
	{
		Write(value.X);
		Write(value.Y);
		Write(value.Z);
	}

	/// <summary>A position from the level's middle, exact however far out, as Origin.ToLevel gives one.</summary>
	public void Write(LevelPosition value)
	{
		Write(value.X);
		Write(value.Y);
		Write(value.Z);
	}

	public void Write(Quaternion value)
	{
		Write(value.X);
		Write(value.Y);
		Write(value.Z);
		Write(value.W);
	}

	/// <summary>A networked Obj, by its id in the session; any other as None.</summary>
	public void Write(Obj value)
	{
		var bits = value.Id;
		Write(value.IsValid ? NativeNet.Call(NativeNet.NetId, (int)(uint)bits, (int)(uint)(bits >> 32)) : 0);
	}

	public void Write(NetPlayer value)
	{
		Write(value.Id);
	}

	/// <summary>Everything written, as bytes.</summary>
	public byte[] ToArray()
	{
		return bytes.ToArray();
	}

	private void Add(ReadOnlySpan<byte> data)
	{
		foreach (var b in data)
		{
			bytes.Add(b);
		}
	}
}
