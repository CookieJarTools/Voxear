using System.Collections.Generic;

namespace Voxear.Tests;

internal sealed class TestTower : Behaviour
{
	[OnValueChanged(nameof(Rebuild))]
	public int Floors = 3;

	public float Height;

	[ValidateInput(nameof(IsColour), "Pick a colour the tower has")]
	[ValueDropdown(nameof(Colours))]
	public string Paint = "Red";

	[Button("Rebuild the tower")]
	private void Rebuild()
	{
		Height = Floors * 2.5f;
	}

	private bool IsColour(string paint)
	{
		return paint == "Red" || paint == "Blue";
	}

	private IEnumerable<ValueDropdownItem> Colours()
	{
		yield return new ValueDropdownItem("Brick red", "Red");
		yield return new ValueDropdownItem("Sky blue", "Blue");
	}
}
