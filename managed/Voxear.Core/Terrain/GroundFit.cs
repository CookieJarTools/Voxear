using System;
using System.Collections.Generic;
using System.Numerics;
using System.Text.Json.Nodes;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Ground Fit: the ground a world of blocks without end makes under and round the Obj's footprint, levelled
/// to its height and eased back to the generator's ground a block a step. Nothing is stored as blocks: move or delete
/// the Obj and the ground comes back as it was. Get it with Obj.GroundFit, null when the Obj has none, or give an Obj
/// one with Obj.AddGroundFit.
/// <code>
/// // A house from a kit, set on a level plot 10 by 8 metres with a step down to the street at its door.
/// var house = Obj.Instantiate(HousePrefab, at);
/// var fit = house.AddGroundFit();
/// fit.Boxes = [new FitBox(Vector2.Zero, new Vector2(10f, 8f))];
/// fit.Level = FitLevel.Point;
/// fit.Point = new Vector3(0f, 0f, 4f);
/// fit.Top = Cobbles;
/// </code>
/// </summary>
public sealed class GroundFit : WholeComponent
{
	internal GroundFit(Obj obj)
		: base(obj, NativeWhole.Whole, NativeWhole.GroundFit)
	{
	}

	/// <summary>How it meets the ground: built up under, levelled, or only cut.</summary>
	public FitMode Mode
	{
		get => Choice("mode", FitMode.Flatten);
		set => Change("mode", PostJson.Of(value));
	}

	/// <summary>Its footprint's boxes, along x and z in metres in the Obj's own space, turned and moved with it.</summary>
	public IReadOnlyList<FitBox> Boxes
	{
		get
		{
			if (Settings()["boxes"] is not JsonArray read)
			{
				return [new FitBox(Vector2.Zero, new Vector2(4f, 4f))];
			}
			var boxes = new List<FitBox>();
			foreach (var node in read)
			{
				var centre = node?["centre"] is JsonArray { Count: 2 } c ? new Vector2(c[0]!.GetValue<float>(), c[1]!.GetValue<float>()) : Vector2.Zero;
				var size = node?["size"] is JsonArray { Count: 2 } s ? new Vector2(s[0]!.GetValue<float>(), s[1]!.GetValue<float>()) : new Vector2(4f, 4f);
				boxes.Add(new FitBox(centre, size));
			}
			return boxes;
		}
		set
		{
			var boxes = new JsonArray();
			foreach (var box in value)
			{
				boxes.Add(new JsonObject { ["centre"] = PostJson.Of(box.Centre), ["size"] = PostJson.Of(box.Size) });
			}
			Change("boxes", boxes);
		}
	}

	/// <summary>An outline besides its boxes, along x and z in metres in the Obj's own space; empty for none.</summary>
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

	/// <summary>What height it levels to: its Obj's, or its Point's.</summary>
	public FitLevel Level
	{
		get => Choice("level", FitLevel.Obj);
		set => Change("level", PostJson.Of(value));
	}

	/// <summary>The point it levels to, in the Obj's own space, where Level is Point: its door.</summary>
	public Vector3 Point
	{
		get => Three("point", Vector3.Zero);
		set => Change("point", PostJson.Of(value));
	}

	/// <summary>How many blocks into the ground it stands.</summary>
	public int Sink
	{
		get => (int)Number("sink", 0f);
		set => Change("sink", value);
	}

	/// <summary>How far past the footprint the ground is levelled, in blocks.</summary>
	public float Margin
	{
		get => Number("margin", 1f);
		set => Change("margin", value);
	}

	/// <summary>Over how many blocks the ground eases back to the generator's, a block a step; 0 for a sheer wall.</summary>
	public int SlopeOut
	{
		get => (int)Number("slope_out", 4f);
		set => Change("slope_out", Math.Max(0, value));
	}

	/// <summary>What made ground is built of; Block.None for its biome's own.</summary>
	public Block Fill
	{
		get => BlockAt("fill");
		set => Change("fill", value.IsValid ? value.Key : null);
	}

	/// <summary>What the levelled ground is topped with; Block.None for its biome's own.</summary>
	public Block Top
	{
		get => BlockAt("top");
		set => Change("top", value.IsValid ? value.Key : null);
	}

	/// <summary>Whether generated trees, plants and structures are left out of its reach.</summary>
	public bool ClearPlants
	{
		get => Switch("clear_plants", true);
		set => Change("clear_plants", value);
	}

	/// <summary>Where two fits meet, the higher priority's ground wins.</summary>
	public int Priority
	{
		get => (int)Number("priority", 0f);
		set => Change("priority", value);
	}

	private Block BlockAt(string key)
	{
		return Settings()[key] is JsonValue value && value.TryGetValue<string>(out var guid) ? Block.FromKey(guid) : Block.None;
	}

	internal static GroundFit? Of(Obj obj)
	{
		var fit = new GroundFit(obj);
		return fit.IsValid ? fit : null;
	}

	public override string ToString()
	{
		return $"{Obj}'s Ground Fit";
	}
}
