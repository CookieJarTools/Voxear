namespace Voxear;

/// <summary>How a graded Road climbs: whole blocks, or a slab on each step where the world has slabs.</summary>
public enum RoadSteps
{
	/// <summary>A whole block a step.</summary>
	Blocks,

	/// <summary>A slab of the road's block before each step up.</summary>
	Slabs,
}
