namespace Voxear;

/// <summary>What a Nav Area marks the ground in its box as, for navigation to cost.</summary>
public enum NavAreaMark
{
	/// <summary>As the blocks are: plain ground, water waded or swum, a door.</summary>
	Auto,

	/// <summary>Plain ground.</summary>
	Walkable,

	/// <summary>Ground sought out: a road, a path through a town.</summary>
	Road,

	/// <summary>Ground walked as if wading.</summary>
	Water,

	/// <summary>Ground nothing walks on: lava, spikes, a crop field.</summary>
	NotWalkable,

	/// <summary>The project's own first area.</summary>
	Custom1,

	/// <summary>The project's own second area.</summary>
	Custom2,

	/// <summary>The project's own third area.</summary>
	Custom3,

	/// <summary>The project's own fourth area.</summary>
	Custom4,
}
