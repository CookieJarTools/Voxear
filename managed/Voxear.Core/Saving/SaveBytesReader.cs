using System;

namespace Voxear.Saving
{
	/// <summary>Reads forward through the bytes, throwing FormatException rather than reading past their end.</summary>
	internal ref struct SaveBytesReader
	{
		private ReadOnlySpan<byte> rest;

		public SaveBytesReader(ReadOnlySpan<byte> bytes)
		{
			rest = bytes;
		}

		public readonly bool AtEnd => rest.IsEmpty;

		public byte Byte()
		{
			return Take(1)[0];
		}

		public ReadOnlySpan<byte> Take(int count)
		{
			if (count < 0 || count > rest.Length)
			{
				throw new FormatException("It is cut short.");
			}

			var taken = rest[..count];
			rest = rest[count..];
			return taken;
		}

		public ulong Varint()
		{
			ulong value = 0;
			for (var shift = 0; shift < 64; shift += 7)
			{
				var next = Byte();
				value |= (ulong)(next & 0x7F) << shift;
				if (next < 0x80)
				{
					return value;
				}
			}
			throw new FormatException("It has a number longer than any save writes.");
		}

		/// <summary>A count or a length, which can be no more than the bytes left.</summary>
		public int Count()
		{
			var count = Varint();
			return count <= (ulong)rest.Length ? (int)count : throw new FormatException("It is cut short.");
		}
	}
}
