namespace Voxear;

/// <summary>Whether the game asks for HDR output, and which kind.</summary>
public enum HdrMode
{
	/// <summary>SDR, as every display shows.</summary>
	Off,

	/// <summary>HDR where the display offers it, HDR10 first, and SDR where it does not.</summary>
	On,

	/// <summary>HDR10 alone: 10 bits a channel, the PQ curve and BT.2020 colours.</summary>
	Hdr10,

	/// <summary>scRGB alone: half floats in linear sRGB colours, past 1 for brighter than paper white.</summary>
	ScRgb,
}
