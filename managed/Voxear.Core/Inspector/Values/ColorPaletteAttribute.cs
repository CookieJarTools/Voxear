using System;

namespace Voxear;

/// <summary>
/// Offers colours to pick for a Color field, as swatches under it, given as hex as a design tool writes them:
/// [ColorPalette("#e63946", "#f1faee", "#457b9d")].
/// </summary>
[AttributeUsage(AttributeTargets.Field)]
public sealed class ColorPaletteAttribute : Attribute
{
	public ColorPaletteAttribute(params string[] colors)
	{
		Colors = colors;
	}

	public string[] Colors { get; }
}
