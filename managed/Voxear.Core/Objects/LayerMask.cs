using System;

namespace Voxear;

/// <summary>
/// Some of the 32 layers, as Unity's LayerMask: a bit for each, by layer number. A ray or a sphere asks after the
/// layers in a mask only. The project names its layers in voxear.toml, and Everything holds them all.
/// <code>
/// var ground = LayerMask.GetMask("Ground", "Platforms");
/// </code>
/// </summary>
public readonly struct LayerMask : IEquatable<LayerMask>
{
	public static readonly LayerMask Everything = new(-1);
	public static readonly LayerMask Nothing = new(0);

	public LayerMask(int value)
	{
		Value = value;
	}

	/// <summary>A bit for each layer in it, by layer number.</summary>
	public int Value { get; }

	/// <summary>A mask of the layers with these names; a name the project does not have is left out.</summary>
	public static LayerMask GetMask(params string[] names)
	{
		var value = 0;
		foreach (var name in names)
		{
			var layer = NameToLayer(name);
			if (layer >= 0)
			{
				value |= 1 << layer;
			}
		}
		return new LayerMask(value);
	}

	/// <summary>The layer with a name, or -1 when the project has none by it.</summary>
	public static int NameToLayer(string name)
	{
		return Interop.NativeLayers.Find(name);
	}

	/// <summary>A layer's name, empty for one the project does not use.</summary>
	public static string LayerToName(int layer)
	{
		return layer is >= 0 and < 32 ? Interop.NativeLayers.Name(layer) : string.Empty;
	}

	public bool Contains(int layer)
	{
		return layer is >= 0 and < 32 && (Value & (1 << layer)) != 0;
	}

	public static implicit operator int(LayerMask mask)
	{
		return mask.Value;
	}

	public static implicit operator LayerMask(int value)
	{
		return new LayerMask(value);
	}

	public static bool operator ==(LayerMask left, LayerMask right)
	{
		return left.Value == right.Value;
	}

	public static bool operator !=(LayerMask left, LayerMask right)
	{
		return left.Value != right.Value;
	}

	public bool Equals(LayerMask other)
	{
		return Value == other.Value;
	}

	public override bool Equals(object? other)
	{
		return other is LayerMask mask && Equals(mask);
	}

	public override int GetHashCode()
	{
		return Value;
	}
}
