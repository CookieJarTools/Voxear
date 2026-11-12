using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A particle effect: several emitters played as one, as an explosion is its flash, its fireball, its smoke, its
/// sparks and what it throws. One of the starters every project has, such as ParticleEffect.Explosion, or one of the
/// project's .particles files. A ParticleEffect field is picked in the Inspector. Play one at a place with
/// Particles.Play, or give it to a Particle System as its Effect.
/// <code>
/// public ParticleEffect Impact;
///
/// // Where the shot lands, thrown out from the surface it hit.
/// Particles.Play(Impact, hit.Point, hit.Normal);
/// </code>
/// </summary>
public readonly struct ParticleEffect : IEquatable<ParticleEffect>, IAsset<ParticleEffect>
{
	private const string Builtin = "builtin:";

	public static readonly ParticleEffect None = default;

	private static readonly string[] starters =
	[
		"fire", "smoke", "sparks", "dust", "rain", "snow", "blood", "muzzle_flash", "explosion", "magic", "mud",
	];

	/// <summary>The effect's GUID, or a starter's name after "builtin:"; away from the engine, its path.</summary>
	private readonly string? key;

	private ParticleEffect(string key)
	{
		this.key = key;
	}

	/// <summary>Flames, the smoke they leave downwind, and embers.</summary>
	public static ParticleEffect Fire => new(Builtin + "fire");

	/// <summary>A column of smoke that drifts with the wind.</summary>
	public static ParticleEffect Smoke => new(Builtin + "smoke");

	/// <summary>A stream of sparks with tails, that bounce where they land.</summary>
	public static ParticleEffect Sparks => new(Builtin + "sparks");

	/// <summary>Dust hanging over a patch of ground.</summary>
	public static ParticleEffect Dust => new(Builtin + "dust");

	/// <summary>Rain falling from a box 14 metres across at its place, so play it overhead.</summary>
	public static ParticleEffect Rain => new(Builtin + "rain");

	/// <summary>Snow falling from a box 14 metres across at its place, so play it overhead.</summary>
	public static ParticleEffect Snow => new(Builtin + "snow");

	/// <summary>A spurt that lands and stays a moment.</summary>
	public static ParticleEffect Blood => new(Builtin + "blood");

	/// <summary>A flash, sparks and a wisp of smoke, once.</summary>
	public static ParticleEffect MuzzleFlash => new(Builtin + "muzzle_flash");

	/// <summary>Flash, fireball, smoke, sparks, tumbling debris and a shockwave, once.</summary>
	public static ParticleEffect Explosion => new(Builtin + "explosion");

	/// <summary>Glowing motes that draw threads of light, round a pulsing light.</summary>
	public static ParticleEffect Magic => new(Builtin + "magic");

	/// <summary>Clods and spatter thrown up, once.</summary>
	public static ParticleEffect Mud => new(Builtin + "mud");

	public bool IsValid => !string.IsNullOrEmpty(key);

	/// <summary>Whether it is one of the starters every project has, rather than one of the project's files.</summary>
	public bool IsStarter => Key.StartsWith(Builtin, StringComparison.Ordinal);

	internal string Key => key ?? string.Empty;

	string? IAsset<ParticleEffect>.AssetGuid => IsValid ? key : null;

	/// <summary>
	/// An effect by project path or GUID, for example "Effects/Blast.particles", or a starter by its name, as
	/// "builtin:explosion". Logs an error and returns None when the project has no such effect.
	/// </summary>
	public static ParticleEffect Load(string path)
	{
		if (string.IsNullOrEmpty(path))
		{
			return None;
		}

		if (path.StartsWith(Builtin, StringComparison.Ordinal))
		{
			return Starter(path);
		}

		var found = Native.IsConnected ? NativeSaves.Key(NativeSaves.KeyReference, 0, path) : path;
		if (found == null)
		{
			Log.Error($"The project has no particle effect \"{path}\".");
			return None;
		}
		return new ParticleEffect(found);
	}

	/// <summary>Plays it at a place, as Particles.Play does.</summary>
	public ParticleSystem Play(Vector3 position)
	{
		return Particles.Play(this, position);
	}

	internal static ParticleEffect FromKey(string? key)
	{
		return string.IsNullOrEmpty(key) ? None : new ParticleEffect(key);
	}

	private static ParticleEffect Starter(string reference)
	{
		if (Array.IndexOf(starters, reference[Builtin.Length..]) >= 0)
		{
			return new ParticleEffect(reference);
		}

		Log.Error($"\"{reference}\" is not a starter effect; they are {Builtin}{string.Join(", " + Builtin, starters)}.");
		return None;
	}

	public static bool operator ==(ParticleEffect left, ParticleEffect right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(ParticleEffect left, ParticleEffect right)
	{
		return !left.Equals(right);
	}

	public bool Equals(ParticleEffect other)
	{
		return Key == other.Key;
	}

	public override bool Equals(object? other)
	{
		return other is ParticleEffect effect && Equals(effect);
	}

	public override int GetHashCode()
	{
		return Key.GetHashCode();
	}

	public override string ToString()
	{
		return IsValid ? $"ParticleEffect({Key})" : "ParticleEffect(none)";
	}
}
