using System;
using System.Collections.Generic;
using System.Numerics;
using System.Text.Json.Nodes;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Area Fill: a square, a plaza or a yard laid on the ground of a world of blocks without end, an outline
/// paved with a mix or a pattern of blocks, level at its Obj's height or on the ground as it is. Get it with
/// Obj.AreaFill, or give an Obj one with Obj.AddAreaFill.
/// </summary>
public sealed class AreaFill : WholeComponent
{
	internal AreaFill(Obj obj)
		: base(obj, NativeWhole.Whole, NativeWhole.AreaFill)
	{
	}

	/// <summary>Its outline along x and z, in metres in the Obj's own space.</summary>
	public IReadOnlyList<Vector2> Outline
	{
		get
		{
			var outline = new List<Vector2>();
			if (Settings()["outline"] is JsonArray read)
			{
				foreach (var node in read)
				{
					if (node is JsonArray { Count: 2 } point)
					{
						outline.Add(new Vector2(point[0]!.GetValue<float>(), point[1]!.GetValue<float>()));
					}
				}
			}
			return outline;
		}
		set
		{
			var outline = new JsonArray();
			foreach (var point in value)
			{
				outline.Add(PostJson.Of(point));
			}
			Change("outline", outline);
		}
	}

	/// <summary>The blocks it is paved with, each as often as its weight.</summary>
	public IReadOnlyList<MixBlock> Mix
	{
		get => MixJson.Mix(Settings());
		set => Change("mix", MixJson.Of(value));
	}

	/// <summary>A small picture of the Mix's entries by place from 1, a row a block along z and a character a block along x, repeated.</summary>
	public IReadOnlyList<string> Pattern
	{
		get => MixJson.Pattern(Settings());
		set => Change("pattern", MixJson.Of(value));
	}

	/// <summary>Level at its Obj's height, or on the ground as it is.</summary>
	public AreaLevel Level
	{
		get => Choice("level", AreaLevel.Follow);
		set => Change("level", PostJson.Of(value));
	}

	/// <summary>What made ground is built of; Block.None for its biome's own.</summary>
	public Block Fill
	{
		get => MixJson.BlockAt(Settings(), "fill");
		set => Change("fill", value.IsValid ? value.Key : null);
	}

	/// <summary>Over how many blocks the ground eases back from it.</summary>
	public int SlopeOut
	{
		get => (int)Number("slope_out", 3f);
		set => Change("slope_out", Math.Max(0, value));
	}

	/// <summary>Whether generated trees, plants and structures are left out of it.</summary>
	public bool ClearPlants
	{
		get => Switch("clear_plants", true);
		set => Change("clear_plants", value);
	}

	/// <summary>Where two meet, the higher priority's ground wins.</summary>
	public int Priority
	{
		get => (int)Number("priority", 0f);
		set => Change("priority", value);
	}

	internal static AreaFill? Of(Obj obj)
	{
		var area = new AreaFill(obj);
		return area.IsValid ? area : null;
	}

	public override string ToString()
	{
		return $"{Obj}'s Area Fill";
	}
}
