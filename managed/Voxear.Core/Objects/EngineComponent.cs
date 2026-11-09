namespace Voxear;

/// <summary>
/// An engine component that can be switched off, as Unity's <c>enabled</c>: for Obj.IsEnabled and Obj.SetEnabled.
/// Rigidbodies and Joints have no switch, as in Unity.
/// </summary>
public enum EngineComponent
{
	MeshRenderer,
	ModelRenderer,
	Light,
	Camera,
	AudioSource,
	AudioListener,
	LodGroup,
	Collider,
	CharacterController,
	Collider2D,
	Vehicle,
	Wheel,
	PostVolume,
	WindZone,
	Seasons,
	FoliageInteraction,
	Foliage,
	TrackMaker,
}
