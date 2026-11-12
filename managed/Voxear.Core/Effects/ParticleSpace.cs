namespace Voxear;

/// <summary>What a Particle System's particles move in.</summary>
public enum ParticleSpace
{
	/// <summary>Their Obj's own space: they move and turn with it, as a torch's flame stays on the torch.</summary>
	Local,

	/// <summary>The world: once out they stay where they are as the Obj moves on, as the smoke a torch leaves behind.</summary>
	World,
}
