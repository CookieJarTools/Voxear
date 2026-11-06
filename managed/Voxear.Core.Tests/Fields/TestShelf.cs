using System.Collections.Generic;

namespace Voxear.Tests;

internal sealed class TestShelf : Behaviour
{
	[TableList]
	public List<Wave> Waves = new List<Wave>();

	[ListDrawerSettings(NumberOfItemsPerPage = 3, ShowIndexLabels = true)]
	public List<int> Scores = new List<int>();

	[ColorPalette("#e63946", "#457b9d")]
	public Color Paint = Color.White;

	[ColorUsage(hdr: true)]
	public Color Glow = Color.Red.WithIntensity(2f);
}
