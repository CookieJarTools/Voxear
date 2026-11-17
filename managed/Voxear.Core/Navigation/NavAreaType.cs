namespace Voxear;

/// <summary>
/// An area of ground, as navigation costs it: what a block's Nav Area or a Nav Area box marks a place as, or what
/// it is by itself. Project Settings ▸ Navigation sets what each costs, and names the four of your own.
/// </summary>
public enum NavAreaType
{
	/// <summary>Plain ground.</summary>
	Walkable,

	/// <summary>Ground sought out: a road, a path through a town.</summary>
	Road,

	/// <summary>Water a block deep, waded.</summary>
	Water,

	/// <summary>Water too deep to stand in, swum by what swims.</summary>
	Swim,

	/// <summary>A shut door, opened by what opens doors.</summary>
	Door,

	/// <summary>Ground nothing stands on.</summary>
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
