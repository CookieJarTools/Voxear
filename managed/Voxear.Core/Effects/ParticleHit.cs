using System.Numerics;

namespace Voxear;

/// <summary>
/// A particle striking what the camera sees, from a Particle System with Hit Events on: told to the behaviours on
/// the system's Obj as OnParticleHit, and to everything listening to Particles.Hit. It comes a few frames after the
/// strike, as the graphics card works it out.
/// </summary>
public readonly struct ParticleHit
{
	internal ParticleHit(Obj obj, int emitter, Vector3 point, Vector3 normal, float speed)
	{
		Obj = obj;
		Emitter = emitter;
		Point = point;
		Normal = normal;
		Speed = speed;
	}

	/// <summary>The Obj of the Particle System the particle came from.</summary>
	public Obj Obj { get; }

	/// <summary>The Particle System the particle came from.</summary>
	public ParticleSystem System => Obj.Particles;

	/// <summary>Which of its effect's emitters, counting from 0 in the effect's order; 0 for a system of its own settings.</summary>
	public int Emitter { get; }

	/// <summary>Where it struck, in the world.</summary>
	public Vector3 Point { get; }

	/// <summary>The way the surface it struck faces there.</summary>
	public Vector3 Normal { get; }

	/// <summary>How fast it was going into the surface, in metres a second: for how loud a sound to play.</summary>
	public float Speed { get; }

	public override string ToString()
	{
		return $"ParticleHit({Obj} at {Point}, {Speed:0.0} m/s)";
	}
}
