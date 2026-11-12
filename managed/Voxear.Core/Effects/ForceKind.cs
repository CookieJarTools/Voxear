namespace Voxear;

/// <summary>What a Force Field does to the particles within its reach.</summary>
public enum ForceKind
{
	/// <summary>Pulls them toward its Obj.</summary>
	Attract,

	/// <summary>Drives them away from its Obj.</summary>
	Repel,

	/// <summary>Swirls them round its Obj's Y axis.</summary>
	Vortex,

	/// <summary>Blows them along its Obj's forward (Z), as a fan or a draught does.</summary>
	Blow,
}
