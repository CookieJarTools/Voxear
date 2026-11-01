using System;
using System.Buffers.Binary;
using System.Collections.Generic;
using System.IO;
using System.Text;

namespace Voxear.Saving
{
	/// <summary>
	/// Saves as compact bytes: "VSB" and a version byte, then every name and text the save uses, each once, then the
	/// values. A value is a tag byte and what follows it: whole numbers as zigzag varints, floats as four bytes when that
	/// loses nothing and eight when it would, and names and texts as their place in the list at the start. Tables and
	/// lists give their count first.
	/// </summary>
	internal sealed class BinarySaveFormat : SaveFormat
	{
		private const byte Version = 1;
		private const byte TagNull = 0;
		private const byte TagFalse = 1;
		private const byte TagTrue = 2;
		private const byte TagInteger = 3;
		private const byte TagFloat32 = 4;
		private const byte TagFloat64 = 5;
		private const byte TagText = 6;
		private const byte TagBytes = 7;
		private const byte TagList = 8;
		private const byte TagTable = 9;
		private const int DeepestNesting = 256;

		private static ReadOnlySpan<byte> Magic => "VSB"u8;

		public override string Name => "binary";

		public override byte[] Write(SaveNode root)
		{
			var strings = new Dictionary<string, int>();
			var order = new List<string>();
			Collect(root, strings, order);

			using var stream = new MemoryStream();
			stream.Write(Magic);
			stream.WriteByte(Version);
			WriteVarint(stream, (ulong)order.Count);
			foreach (var text in order)
			{
				var bytes = Encoding.UTF8.GetBytes(text);
				WriteVarint(stream, (ulong)bytes.Length);
				stream.Write(bytes);
			}
			WriteNode(stream, root, strings);
			return stream.ToArray();
		}

		public override SaveNode Read(ReadOnlySpan<byte> bytes)
		{
			if (bytes.Length < 4 || !bytes[..3].SequenceEqual(Magic))
			{
				throw new FormatException("It is not a binary save.");
			}
			if (bytes[3] > Version)
			{
				throw new FormatException($"It is binary version {bytes[3]}, from a newer Voxear.");
			}

			var reader = new SaveBytesReader(bytes[4..]);
			var count = reader.Count();
			var strings = new string[count];
			for (var at = 0; at < count; at++)
			{
				strings[at] = Encoding.UTF8.GetString(reader.Take(reader.Count()));
			}
			var root = ReadNode(ref reader, strings, 0);
			if (!reader.AtEnd)
			{
				throw new FormatException("It has more bytes after its last value.");
			}
			return root;
		}

		private static void Collect(SaveNode node, Dictionary<string, int> strings, List<string> order)
		{
			if (node.Kind == SaveNodeKind.Text)
			{
				Intern((string)node.Value!, strings, order);
			}
			foreach (var item in node.Items)
			{
				Collect(item, strings, order);
			}
			foreach (var field in node.Fields)
			{
				Intern(field.Key, strings, order);
				Collect(field.Value, strings, order);
			}
		}

		private static void Intern(string text, Dictionary<string, int> strings, List<string> order)
		{
			if (strings.TryAdd(text, order.Count))
			{
				order.Add(text);
			}
		}

		private static void WriteNode(Stream stream, SaveNode node, Dictionary<string, int> strings)
		{
			switch (node.Kind)
			{
				case SaveNodeKind.Null:
					stream.WriteByte(TagNull);
					break;
				case SaveNodeKind.Bool:
					stream.WriteByte((bool)node.Value! ? TagTrue : TagFalse);
					break;
				case SaveNodeKind.Integer:
					var number = (long)node.Value!;
					stream.WriteByte(TagInteger);
					WriteVarint(stream, (ulong)((number << 1) ^ (number >> 63)));
					break;
				case SaveNodeKind.Float:
					WriteFloat(stream, (double)node.Value!);
					break;
				case SaveNodeKind.Text:
					stream.WriteByte(TagText);
					WriteVarint(stream, (ulong)strings[(string)node.Value!]);
					break;
				case SaveNodeKind.Bytes:
					var bytes = (byte[])node.Value!;
					stream.WriteByte(TagBytes);
					WriteVarint(stream, (ulong)bytes.Length);
					stream.Write(bytes);
					break;
				case SaveNodeKind.List:
					stream.WriteByte(TagList);
					WriteVarint(stream, (ulong)node.Count);
					foreach (var item in node.Items)
					{
						WriteNode(stream, item, strings);
					}
					break;
				default:
					stream.WriteByte(TagTable);
					WriteVarint(stream, (ulong)node.Count);
					foreach (var field in node.Fields)
					{
						WriteVarint(stream, (ulong)strings[field.Key]);
						WriteNode(stream, field.Value, strings);
					}
					break;
			}
		}

		/// <summary>Four bytes when the number survives the trip through a float, as every float field's does.</summary>
		private static void WriteFloat(Stream stream, double number)
		{
			Span<byte> bytes = stackalloc byte[8];
			var single = (float)number;
			if (single == number || double.IsNaN(number))
			{
				stream.WriteByte(TagFloat32);
				BinaryPrimitives.WriteSingleLittleEndian(bytes, single);
				stream.Write(bytes[..4]);
				return;
			}
			stream.WriteByte(TagFloat64);
			BinaryPrimitives.WriteDoubleLittleEndian(bytes, number);
			stream.Write(bytes);
		}

		private static void WriteVarint(Stream stream, ulong value)
		{
			while (value >= 0x80)
			{
				stream.WriteByte((byte)(value | 0x80));
				value >>= 7;
			}
			stream.WriteByte((byte)value);
		}

		private static SaveNode ReadNode(ref SaveBytesReader reader, string[] strings, int depth)
		{
			if (depth > DeepestNesting)
			{
				throw new FormatException("Its values are nested deeper than any save writes them.");
			}

			var tag = reader.Byte();
			switch (tag)
			{
				case TagNull:
					return SaveNode.Null;
				case TagFalse:
					return false;
				case TagTrue:
					return true;
				case TagInteger:
					var zigzag = reader.Varint();
					return SaveNode.Integer((long)(zigzag >> 1) ^ -(long)(zigzag & 1));
				case TagFloat32:
					return BinaryPrimitives.ReadSingleLittleEndian(reader.Take(4));
				case TagFloat64:
					return BinaryPrimitives.ReadDoubleLittleEndian(reader.Take(8));
				case TagText:
					return SaveNode.Text(TextAt(ref reader, strings));
				case TagBytes:
					return SaveNode.Bytes(reader.Take(reader.Count()).ToArray());
				case TagList:
					var list = SaveNode.NewList();
					for (var count = reader.Count(); count > 0; count--)
					{
						list.Add(ReadNode(ref reader, strings, depth + 1));
					}
					return list;
				case TagTable:
					var table = SaveNode.NewTable();
					for (var count = reader.Count(); count > 0; count--)
					{
						var name = TextAt(ref reader, strings);
						table.AddField(name, ReadNode(ref reader, strings, depth + 1));
					}
					return table;
				default:
					throw new FormatException($"It has a value of an unknown kind ({tag}).");
			}
		}

		private static string TextAt(ref SaveBytesReader reader, string[] strings)
		{
			var at = reader.Varint();
			return at < (ulong)strings.Length ? strings[at] : throw new FormatException("It names text it does not have.");
		}
	}
}
