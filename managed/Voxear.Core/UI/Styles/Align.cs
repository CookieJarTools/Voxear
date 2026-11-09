namespace Voxear;

/// <summary>Where elements sit across the way their box lays them out: AlignItems for all of them, AlignSelf for one.</summary>
public enum Align
{
	/// <summary>At the start: the top of a row, the left of a column.</summary>
	Start,

	/// <summary>In the middle.</summary>
	Center,

	/// <summary>At the end: the bottom of a row, the right of a column.</summary>
	End,

	/// <summary>Stretched to fill, the default.</summary>
	Stretch,

	/// <summary>With their text lined up.</summary>
	Baseline,
}
