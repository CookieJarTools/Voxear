using System;

namespace Voxear;

/// <summary>
/// Says how a Color field is used. With hdr: true, as [ColorUsage(hdr: true)], it can be brighter than white, as an
/// emissive glow or a light's colour is: its picker adds an Intensity in stops, and the colour is kept times two to
/// that power, so a component over 1 means it glows. <see cref="Color.Intensity"/> reads it back.
/// </summary>
[AttributeUsage(AttributeTargets.Field)]
public sealed class ColorUsageAttribute : Attribute
{
	public ColorUsageAttribute(bool hdr = false)
	{
		Hdr = hdr;
	}

	public bool Hdr { get; }
}
