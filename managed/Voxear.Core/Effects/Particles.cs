using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// What moves every Particle System's particles at once. Only systems that take forces are moved, each as much as
/// its Forces says.
/// </summary>
public static unsafe class Particles
{
	/// <summary>
	/// Sets off a blast at a point: particles within its radius are thrown out from it, those at its middle to
	/// <paramref name="speed"/> metres a second, less further out. It is over in a moment.
	/// <code>
	/// // A grenade throws the smoke and dust round it outwards.
	/// Particles.Push(grenade.Position, 6f, 12f);
	/// </code>
	/// </summary>
	public static void Push(Vector3 position, float radius, float speed)
	{
		if (Native.IsConnected)
		{
			Native.Api->ParticleBlast(&position, radius, speed);
		}
	}
}
