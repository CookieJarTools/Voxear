using System;
using System.Collections.Generic;
using System.Numerics;
using System.Text.Json.Nodes;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Road: a road, a path or a canal laid on the ground of a world of blocks without end along its points, as
/// wide as each says, with a mix or a pattern of blocks, an edge and a kerb; graded between its points or laid on the
/// ground, over gaps as a bridge and through hills as a tunnel. Nothing is stored as blocks: move or delete the Obj
/// and the ground comes back. Get it with Obj.Road, or give an Obj one with Obj.AddRoad.
/// <code>
/// var road = Obj.AddRoad();
/// road.Points = [new RoadPoint(Vector3.Zero, 5f), new RoadPoint(new Vector3(0f, 1f, 40f), 9f)];
/// road.Mix = [new MixBlock(Cobbles, 3), new MixBlock(Gravel, 1)];
/// road.Edge = Kerbstone;
/// </code>
/// </summary>
public sealed class Road : WholeComponent
{
	internal Road(Obj obj)
		: base(obj, NativeWhole.Whole, NativeWhole.Road)
	{
	}

	/// <summary>Its points, in the Obj's own space, each with how wide the road is there in blocks.</summary>
	public IReadOnlyList<RoadPoint> Points
	{
		get
		{
			var points = new List<RoadPoint>();
			if (Settings()["points"] is JsonArray read)
			{
				foreach (var node in read)
				{
					var width = node?["width"] is JsonValue w && w.TryGetValue<float>(out var number) ? number : 5f;
					points.Add(new RoadPoint(MixJson.Three(node?["at"], Vector3.Zero), width));
				}
			}
			return points;
		}
		set
		{
			var points = new JsonArray();
			foreach (var point in value)
			{
				points.Add(new JsonObject { ["at"] = PostJson.Of(point.At), ["width"] = point.Width });
			}
			Change("points", points);
		}
	}

	/// <summary>The blocks it is laid with, each as often as its weight; empty for the ground's own top.</summary>
	public IReadOnlyList<MixBlock> Mix
	{
		get => MixJson.Mix(Settings());
		set => Change("mix", MixJson.Of(value));
	}

	/// <summary>A small picture of the Mix's entries by place from 1, a row a block along and a character a block across, repeated; 0 for the mix.</summary>
	public IReadOnlyList<string> Pattern
	{
		get => MixJson.Pattern(Settings());
		set => Change("pattern", MixJson.Of(value));
	}

	/// <summary>The block along each edge; Block.None for none.</summary>
	public Block Edge
	{
		get => MixJson.BlockAt(Settings(), "edge");
		set => Change("edge", value.IsValid ? value.Key : null);
	}

	/// <summary>How many blocks its kerb stands over the road, along each edge.</summary>
	public int Kerb
	{
		get => (int)Number("kerb", 0f);
		set => Change("kerb", Math.Max(0, value));
	}

	/// <summary>How it meets the ground.</summary>
	public RoadGround Ground
	{
		get => Choice("ground", RoadGround.Grade);
		set => Change("ground", PostJson.Of(value));
	}

	/// <summary>The most it climbs a block along, in blocks, graded.</summary>
	public float Grade
	{
		get => Number("grade", 0.25f);
		set => Change("grade", value);
	}

	/// <summary>Over how many blocks each side the ground eases back, graded.</summary>
	public int SlopeOut
	{
		get => (int)Number("slope_out", 3f);
		set => Change("slope_out", Math.Max(0, value));
	}

	/// <summary>How it climbs: whole blocks, or slabs before each step up.</summary>
	public RoadSteps Steps
	{
		get => Choice("steps", RoadSteps.Blocks);
		set => Change("steps", PostJson.Of(value));
	}

	/// <summary>Whether it crosses ground or water well under it as a bridge on piers.</summary>
	public bool Bridge
	{
		get => Switch("bridge", false);
		set => Change("bridge", value);
	}

	/// <summary>A bridge's deck and piers.</summary>
	public Block Deck
	{
		get => MixJson.BlockAt(Settings(), "deck");
		set => Change("deck", value.IsValid ? value.Key : null);
	}

	/// <summary>Whether it goes through ground well over it as a tunnel.</summary>
	public bool Tunnel
	{
		get => Switch("tunnel", false);
		set => Change("tunnel", value);
	}

	/// <summary>A tunnel's roof, where the ground does not hold one.</summary>
	public Block Roof
	{
		get => MixJson.BlockAt(Settings(), "roof");
		set => Change("roof", value.IsValid ? value.Key : null);
	}

	/// <summary>Water in it, as a canal; Block.None for a road.</summary>
	public Block Water
	{
		get => MixJson.BlockAt(Settings(), "water");
		set => Change("water", value.IsValid ? value.Key : null);
	}

	/// <summary>How many blocks deep a canal is.</summary>
	public int Depth
	{
		get => (int)Number("depth", 2f);
		set => Change("depth", Math.Max(1, value));
	}

	/// <summary>Whether generated trees, plants and structures are left out of its reach.</summary>
	public bool ClearPlants
	{
		get => Switch("clear_plants", true);
		set => Change("clear_plants", value);
	}

	/// <summary>Where two roads meet, the higher priority's is laid.</summary>
	public int Priority
	{
		get => (int)Number("priority", 0f);
		set => Change("priority", value);
	}

	internal static Road? Of(Obj obj)
	{
		var road = new Road(obj);
		return road.IsValid ? road : null;
	}

	public override string ToString()
	{
		return $"{Obj}'s Road";
	}
}
