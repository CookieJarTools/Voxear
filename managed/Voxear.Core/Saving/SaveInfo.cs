using System;
using System.Text.Json;

namespace Voxear.Saving;

/// <summary>
/// What a save says about itself, kept apart from its data so a list of slots reads only this: the format its data
/// is in, the type at its top, its version, and the summary a game gave it.
/// </summary>
internal sealed record SaveInfo(string Format, string? Type, int Version, string? Summary)
{
	public byte[] ToBytes()
	{
		var buffer = new System.Buffers.ArrayBufferWriter<byte>();
		using (var json = new Utf8JsonWriter(buffer))
		{
			json.WriteStartObject();
			json.WriteString("format", Format);
			if (Type != null)
			{
				json.WriteString("type", Type);
			}
			json.WriteNumber("version", Version);
			if (Summary != null)
			{
				json.WriteString("summary", Summary);
			}
			json.WriteEndObject();
		}
		return buffer.WrittenSpan.ToArray();
	}

	public static SaveInfo Parse(byte[] bytes, string slot)
	{
		try
		{
			using var document = JsonDocument.Parse(bytes);
			return From(document.RootElement);
		}
		catch (Exception exception) when (exception is JsonException or InvalidOperationException)
		{
			throw new SaveException(SaveError.Damaged, $"The save \"{slot}\" cannot be read: what it says about itself is not JSON.", exception);
		}
	}

	public static SaveInfo From(JsonElement info)
	{
		if (info.ValueKind != JsonValueKind.Object)
		{
			return new SaveInfo(string.Empty, null, 1, null);
		}

		var format = info.TryGetProperty("format", out var named) ? named.GetString() ?? string.Empty : string.Empty;
		var type = info.TryGetProperty("type", out var typed) ? typed.GetString() : null;
		var version = info.TryGetProperty("version", out var numbered) && numbered.TryGetInt32(out var number) ? number : 1;
		var summary = info.TryGetProperty("summary", out var summed) ? summed.GetString() : null;
		return new SaveInfo(format, type, version, summary);
	}
}
