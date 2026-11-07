using System;
using System.Numerics;
using System.Text;

namespace Voxear;

/// <summary>Reads replicated values back from the bytes a NetWriter wrote. Past the end, everything reads as nothing.</summary>
public sealed class NetReader
{
	private readonly byte[] bytes;
	private int at;

	public NetReader(byte[] bytes)
	{
		this.bytes = bytes;
	}

	public bool ReadBool()
	{
		return Take(1).Length == 1 && bytes[at - 1] != 0;
	}

	public int ReadInt()
	{
		var data = Take(4);
		return data.Length == 4 ? BitConverter.ToInt32(data) : 0;
	}

	public long ReadLong()
	{
		var data = Take(8);
		return data.Length == 8 ? BitConverter.ToInt64(data) : 0;
	}

	public float ReadFloat()
	{
		return BitConverter.Int32BitsToSingle(ReadInt());
	}

	public double ReadDouble()
	{
		return BitConverter.Int64BitsToDouble(ReadLong());
	}

	public string ReadString()
	{
		var length = ReadInt();
		return length <= 0 ? string.Empty : Encoding.UTF8.GetString(Take(length));
	}

	public Vector2 ReadVector2()
	{
		return new Vector2(ReadFloat(), ReadFloat());
	}

	public Vector3 ReadVector3()
	{
		return new Vector3(ReadFloat(), ReadFloat(), ReadFloat());
	}

	public Quaternion ReadQuaternion()
	{
		return new Quaternion(ReadFloat(), ReadFloat(), ReadFloat(), ReadFloat());
	}

	private ReadOnlySpan<byte> Take(int count)
	{
		var taken = Math.Max(0, Math.Min(count, bytes.Length - at));
		var span = new ReadOnlySpan<byte>(bytes, at, taken);
		at += taken;
		return span;
	}
}
