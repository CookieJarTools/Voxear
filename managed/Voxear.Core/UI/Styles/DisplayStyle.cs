namespace Voxear;

/// <summary>How an element lays out what is in it, or whether it shows at all.</summary>
public enum DisplayStyle
{
	/// <summary>In a line, as FlexDirection says, the default.</summary>
	Flex,

	/// <summary>In a grid, as a style sheet's grid properties say.</summary>
	Grid,

	/// <summary>Hidden, taking no space.</summary>
	None,
}
