using System;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// What a Particle System's particles are drawn with: one of the textures every project has, such as
/// ParticleTexture.Puff, or one of the project's pictures, with ParticleTexture.Of.
/// <code>
/// sparks.Texture = ParticleTexture.Flare;
/// smoke.Texture = ParticleTexture.Of(Texture.Load("Effects/Smoke.png"));
/// </code>
/// </summary>
public readonly struct ParticleTexture : IEquatable<ParticleTexture>
{
	private const string Builtin = "builtin:";

	/// <summary>The picture's GUID, or a built-in texture's name after "builtin:".</summary>
	private readonly string? key;

	private ParticleTexture(string key)
	{
		this.key = key;
	}

	/// <summary>A soft round dot, drawn without a picture: sparks, drops, flakes.</summary>
	public static ParticleTexture Dot => new(Builtin + "dot");

	/// <summary>A ragged puff: smoke, dust, steam.</summary>
	public static ParticleTexture Puff => new(Builtin + "puff");

	/// <summary>A tongue of flame.</summary>
	public static ParticleTexture Flame => new(Builtin + "flame");

	/// <summary>A thin ring: a shockwave, a ripple.</summary>
	public static ParticleTexture Ring => new(Builtin + "ring");

	/// <summary>A burst of rays: a muzzle flash, a glint.</summary>
	public static ParticleTexture Flare => new(Builtin + "flare");

	/// <summary>A hard-edged chip: debris, mud, clots.</summary>
	public static ParticleTexture Shard => new(Builtin + "shard");

	/// <summary>Whether it is one of the textures every project has, rather than one of the project's pictures.</summary>
	public bool IsBuiltin => Key.StartsWith(Builtin, StringComparison.Ordinal);

	internal string Key => key ?? Builtin + "dot";

	/// <summary>One of the project's pictures: a sheet of Tiles frames when it holds several.</summary>
	public static ParticleTexture Of(Texture picture)
	{
		var guid = picture.IsSet ? NativeSaves.Key(NativeSaves.KeyReference, 0, picture.Reference) : null;
		return new ParticleTexture(guid ?? picture.Reference);
	}

	internal static ParticleTexture FromKey(string? key)
	{
		return string.IsNullOrEmpty(key) ? Dot : new ParticleTexture(key);
	}

	public static bool operator ==(ParticleTexture left, ParticleTexture right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(ParticleTexture left, ParticleTexture right)
	{
		return !left.Equals(right);
	}

	public bool Equals(ParticleTexture other)
	{
		return Key == other.Key;
	}

	public override bool Equals(object? other)
	{
		return other is ParticleTexture texture && Equals(texture);
	}

	public override int GetHashCode()
	{
		return Key.GetHashCode();
	}

	public override string ToString()
	{
		return Key;
	}
}
