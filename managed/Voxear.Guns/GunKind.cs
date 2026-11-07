namespace Voxear;

/// <summary>How a gun works, which decides how it is held, kicks and is reloaded.</summary>
public enum GunKind
{
	/// <summary>A turn-bolt rifle loaded from stripper clips.</summary>
	BoltActionRifle,

	/// <summary>A self-loading rifle fed by an en-bloc clip.</summary>
	SemiAutoRifle,

	/// <summary>A sub-machine gun that fires from an open bolt, from a box magazine.</summary>
	OpenBoltSmg,

	/// <summary>A pistol with a slide, from a box magazine.</summary>
	SlidePistol,

	/// <summary>A pistol whose breech opens by a toggle.</summary>
	TogglePistol,

	/// <summary>A machine gun fed by a belt under a top cover.</summary>
	BeltFedMg,

	/// <summary>A shotgun loaded a shell at a time into a tube, worked by a pump.</summary>
	PumpShotgun,

	/// <summary>A revolver whose barrel tips down to load it.</summary>
	TopBreakRevolver,
}
