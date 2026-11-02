namespace Voxear;

/// <summary>
/// Where in the frame a system runs, in the engine's order. Each stage's systems run after the behaviours'
/// callbacks of that point in the frame and before the engine's own systems there, so a system in PostUpdate runs
/// after LateUpdate, and what it writes to a Transform shows the same frame.
/// </summary>
public enum Stage
{
	/// <summary>The start of the frame, before Awake, OnEnable and Start run for what is new.</summary>
	First,

	/// <summary>Each fixed step, after FixedUpdate.</summary>
	FixedUpdate,

	/// <summary>After Update. Where systems run unless they say otherwise.</summary>
	Update,

	/// <summary>After LateUpdate, before transforms are worked out for drawing.</summary>
	PostUpdate,

	/// <summary>The end of the frame, after destroyed Objs have gone.</summary>
	Last,
}
