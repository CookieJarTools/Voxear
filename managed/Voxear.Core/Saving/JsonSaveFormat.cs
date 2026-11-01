using System;
using System.Text.Json;

namespace Voxear.Saving
{
	/// <summary>
	/// Saves as indented JSON. Tables are objects and lists are arrays; bytes are base64 text, and numbers JSON cannot
	/// hold (NaN and the infinities) are text, all read back by the type they go into. Reading allows comments and
	/// trailing commas, for saves changed by hand.
	/// </summary>
	internal sealed class JsonSaveFormat : SaveFormat
	{
		private static readonly JsonWriterOptions Writing = new JsonWriterOptions { Indented = true, IndentCharacter = '\t', IndentSize = 1 };
		private static readonly JsonDocumentOptions Reading = new JsonDocumentOptions
		{
			AllowTrailingCommas = true,
			CommentHandling = JsonCommentHandling.Skip,
			MaxDepth = 256,
		};

		public override string Name => "json";

		public override byte[] Write(SaveNode root)
		{
			var buffer = new System.Buffers.ArrayBufferWriter<byte>();
			using (var json = new Utf8JsonWriter(buffer, Writing))
			{
				WriteNode(json, root);
			}
			return buffer.WrittenSpan.ToArray();
		}

		public override SaveNode Read(ReadOnlySpan<byte> bytes)
		{
			try
			{
				var reader = new Utf8JsonReader(bytes, new JsonReaderOptions
				{
					AllowTrailingCommas = Reading.AllowTrailingCommas,
					CommentHandling = Reading.CommentHandling,
					MaxDepth = Reading.MaxDepth,
				});
				using var document = JsonDocument.ParseValue(ref reader);
				return ReadNode(document.RootElement);
			}
			catch (JsonException exception)
			{
				throw new FormatException($"It is not JSON: {exception.Message}", exception);
			}
		}

		private static void WriteNode(Utf8JsonWriter json, SaveNode node)
		{
			switch (node.Kind)
			{
				case SaveNodeKind.Null:
					json.WriteNullValue();
					break;
				case SaveNodeKind.Bool:
					json.WriteBooleanValue((bool)node.Value!);
					break;
				case SaveNodeKind.Integer:
					json.WriteNumberValue((long)node.Value!);
					break;
				case SaveNodeKind.Float:
					WriteFloat(json, (double)node.Value!);
					break;
				case SaveNodeKind.Text:
					json.WriteStringValue((string)node.Value!);
					break;
				case SaveNodeKind.Bytes:
					json.WriteBase64StringValue((byte[])node.Value!);
					break;
				case SaveNodeKind.List:
					json.WriteStartArray();
					foreach (var item in node.Items)
					{
						WriteNode(json, item);
					}
					json.WriteEndArray();
					break;
				default:
					json.WriteStartObject();
					foreach (var field in node.Fields)
					{
						json.WritePropertyName(field.Key);
						WriteNode(json, field.Value);
					}
					json.WriteEndObject();
					break;
			}
		}

		private static void WriteFloat(Utf8JsonWriter json, double number)
		{
			if (double.IsFinite(number))
			{
				json.WriteNumberValue(number);
			}
			else
			{
				json.WriteStringValue(double.IsNaN(number) ? "NaN" : number > 0 ? "Infinity" : "-Infinity");
			}
		}

		private static SaveNode ReadNode(JsonElement element)
		{
			switch (element.ValueKind)
			{
				case JsonValueKind.Null:
				case JsonValueKind.Undefined:
					return SaveNode.Null;
				case JsonValueKind.True:
					return true;
				case JsonValueKind.False:
					return false;
				case JsonValueKind.Number:
					return element.TryGetInt64(out var whole) ? SaveNode.Integer(whole) : element.GetDouble();
				case JsonValueKind.String:
					return SaveNode.Text(element.GetString()!);
				case JsonValueKind.Array:
					var list = SaveNode.NewList();
					foreach (var item in element.EnumerateArray())
					{
						list.Add(ReadNode(item));
					}
					return list;
				default:
					var table = SaveNode.NewTable();
					foreach (var property in element.EnumerateObject())
					{
						table.AddField(property.Name, ReadNode(property.Value));
					}
					return table;
			}
		}
	}
}
