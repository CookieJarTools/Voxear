namespace Voxear;

/// <summary>
/// Where in the frame a system runs, in the engine's order. Each stage's systems run after the behaviours'
/// callbacks of that point in the frame and before the engine's own systems there, so a system in PostUpdate runs
/// after LateUpdate, and what it writes to a Transform shows the same frame. Pose is the one the other way round:
/// its systems run after the engine has posed its own, so they see where the camera and the bones are.
/// </summary>
public enum Stage
{
	/// <summary>The start of the frame, before Awake, OnEnable and Start run for what is new.</summary>
	First = 0,

	/// <summary>Each fixed step, after FixedUpdate.</summary>
	FixedUpdate = 1,

	/// <summary>After Update. Where systems run unless they say otherwise.</summary>
	Update = 2,

	/// <summary>
	/// After animation has posed the bones and behaviours' PoseUpdate has run, before Objs on bones follow them: to
	/// bend bones and place what characters hold.
	/// </summary>
	Pose = 5,

	/// <summary>After LateUpdate, before transforms are worked out for drawing.</summary>
	PostUpdate = 3,

	/// <summary>The end of the frame, after destroyed Objs have gone.</summary>
	Last = 4,
}
