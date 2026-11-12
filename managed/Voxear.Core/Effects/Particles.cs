using System;
using System.Globalization;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// Effects played at a place, and what moves every Particle System's particles at once.
/// <code>
/// // An explosion where the shell lands. Nothing to keep, and nothing to clear up after it.
/// Particles.Play(ParticleEffect.Explosion, shell.WorldPosition);
///
/// // A flash that stays on the muzzle as the gun moves.
/// Particles.Play(ParticleEffect.MuzzleFlash, muzzle);
/// </code>
/// </summary>
public static unsafe class Particles
{
	/// <summary>
	/// Raised for every strike of every Particle System with Hit Events on, played effects among them, after its Obj's
	/// behaviours are told. For one place to play every impact sound from. Take your handler off when its behaviour
	/// is destroyed.
	/// <code>
	/// Particles.Hit += hit => Audio.Play(clink, hit.Point, volume: hit.Speed / 10f);
	/// </code>
	/// </summary>
	public static event Action<ParticleHit>? Hit;

	/// <summary>
	/// Plays an effect at a point in the world, upright. It gets an Obj of its own, which is gone when the effect is
	/// over: once the last particle of one that plays once has died, or for one that loops, such as Fire, once you
	/// Stop it and its last have died. Keep what it returns to stop it, or to move its Obj. None for no effect, or
	/// away from the engine.
	/// </summary>
	public static ParticleSystem Play(ParticleEffect effect, Vector3 position)
	{
		return Play(effect, position, Quaternion.Identity);
	}

	/// <summary>
	/// Plays an effect at a point with its up along a direction, as the normal of the surface a shot hit, so its
	/// sparks and dust are thrown out from the surface.
	/// </summary>
	public static ParticleSystem Play(ParticleEffect effect, Vector3 position, Vector3 up)
	{
		return Play(effect, position, Quaternion.FromToRotation(Vector3.Up, up));
	}

	/// <summary>
	/// Plays an effect at a point, turned. Each play sends out particles of its own; give two plays the same seed and
	/// they send out the same ones, as every player's machine must when a game on a network plays one for them all.
	/// </summary>
	public static ParticleSystem Play(ParticleEffect effect, Vector3 position, Quaternion rotation, int? seed = null)
	{
		return Play(effect, Obj.None, position, rotation, seed);
	}

	/// <summary>
	/// Plays an effect on an Obj: at its middle, turned as it is, and going where it goes, as a flash on a muzzle or
	/// flames on a torch. It goes when the Obj does.
	/// </summary>
	public static ParticleSystem Play(ParticleEffect effect, Obj on, int? seed = null)
	{
		return on.IsValid ? Play(effect, on, Vector3.Zero, Quaternion.Identity, seed) : ParticleSystem.None;
	}

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

	internal static void Raise(ParticleHit hit)
	{
		try
		{
			Hit?.Invoke(hit);
		}
		catch (Exception exception)
		{
			Log.Exception(exception);
		}
	}

	private static ParticleSystem Play(ParticleEffect effect, Obj under, Vector3 at, Quaternion turned, int? seed)
	{
		if (!effect.IsValid)
		{
			return ParticleSystem.None;
		}

		// As the engine reads it: the effect, where, how it is turned and its seed, a line each.
		var asked = string.Create(
			CultureInfo.InvariantCulture,
			$"{effect.Key}\n{at.X:R} {at.Y:R} {at.Z:R}\n{turned.X:R} {turned.Y:R} {turned.Z:R} {turned.W:R}\n{(uint?)seed}");
		var made = NativeParticles.PlayAt(under, asked);
		return made.IsValid ? made.Particles : ParticleSystem.None;
	}
}
