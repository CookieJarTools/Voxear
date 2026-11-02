namespace Voxear.Scripting;

/// <summary>Lifecycle phases, numbered as in crates/voxear_scene/src/lifecycle.rs.</summary>
internal enum Phase
{
	Awake = 0,
	OnEnable = 1,
	Start = 2,
	FixedUpdate = 3,
	Update = 4,
	LateUpdate = 5,
	OnDisable = 6,
	OnDestroy = 7,
}
