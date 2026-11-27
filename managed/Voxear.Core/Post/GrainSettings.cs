using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>Film grain: fine noise over the picture, new each frame, strongest in the shadows and mid-tones.</summary>
public sealed class GrainSettings
{
	/// <summary>How strong it is, from 0 (off) to 1: 0.1 is a hint, 0.4 a grainy film.</summary>
	public float Intensity;

	/// <summary>How large each speck is, in pixels, from 1 to 4.</summary>
	public float Size = 1.5f;

	/// <summary>How coloured the specks are, from 0 (grey) to 1.</summary>
	public float Colour = 0.3f;

	internal void Read(JsonObject from)
	{
		PostJson.Read(from, "intensity", ref Intensity);
		PostJson.Read(from, "size", ref Size);
		PostJson.Read(from, "colour", ref Colour);
	}

	internal void Write(JsonObject to)
	{
		to["intensity"] = Intensity;
		to["size"] = Size;
		to["colour"] = Colour;
	}
}
