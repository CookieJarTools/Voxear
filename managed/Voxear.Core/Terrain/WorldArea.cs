using System.Collections.Generic;
using System.Numerics;
using System.Text.Json.Nodes;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's World Area: a part of a world, its outline round the Obj or a patch painted on the world's Area map,
/// between two heights, with tags. Each tag carries a data asset of the game's own, read with
/// <see cref="Areas.Value{T}"/>. A structure's placement keeps within areas of some tags and out of others, and an
/// area with No Structures keeps every structure out. Get it with Obj.WorldArea, or give an Obj one with
/// Obj.AddWorldArea.
/// <code>
/// var town = Obj.AddWorldArea();
/// town.Outline = [new(-60f, -60f), new(60f, -60f), new(60f, 60f), new(-60f, 60f)];
/// town.Tags = [new AreaEntry(Town, Laws)];
/// town.NoStructures = true;
/// </code>
/// </summary>
public sealed class WorldArea : WholeComponent
{
	internal WorldArea(Obj obj)
		: base(obj, NativeWhole.Whole, NativeWhole.WorldArea)
	{
	}

	/// <summary>Whether it is its outline, or what is painted with its number.</summary>
	public AreaShape Shape
	{
		get => Choice("shape", AreaShape.Outline);
		set => Change("shape", PostJson.Of(value));
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

	/// <summary>Its number on the world's painted Area map, given it the first time it is painted; 0 before.</summary>
	public int PaintNumber => (int)Number("number", 0f);

	/// <summary>The lowest it holds, in metres up in the world; null for any.</summary>
	public float? Low
	{
		get => Settings()["low"] is JsonValue value && value.TryGetValue<float>(out var low) ? low : null;
		set => Change("low", value);
	}

	/// <summary>The highest it holds, in metres up in the world; null for any.</summary>
	public float? High
	{
		get => Settings()["high"] is JsonValue value && value.TryGetValue<float>(out var high) ? high : null;
		set => Change("high", value);
	}

	/// <summary>Which comes first where two hold a place: the higher.</summary>
	public int Priority
	{
		get => (int)Number("priority", 0f);
		set => Change("priority", value);
	}

	/// <summary>Whether it keeps the generator's structures out.</summary>
	public bool NoStructures
	{
		get => Switch("no_structures", false);
		set => Change("no_structures", value);
	}

	/// <summary>Its tags, each with the data asset it carries here.</summary>
	public IReadOnlyList<AreaEntry> Tags
	{
		get => AreaJson.Entries(Settings());
		set => Change("tags", AreaJson.Of(value));
	}

	/// <summary>Paints it on the world's Area map round <paramref name="at"/>, or takes it off there.</summary>
	public bool Paint(Vector3 at, float radius = 20f, bool takeAway = false)
	{
		var asked = new JsonObject
		{
			["ask"] = "paint",
			["map"] = "area",
			["area"] = Obj.Id,
			["at"] = PostJson.Of(at),
			["radius"] = radius,
			["falloff"] = 0f,
			["mode"] = takeAway ? "take_away" : "add",
		};
		return NativePost.Call(NativePost.WorldAsk, asked.ToJsonString()) == "true";
	}

	internal static WorldArea? Of(Obj obj)
	{
		var area = new WorldArea(obj);
		return area.IsValid ? area : null;
	}

	public override string ToString()
	{
		return $"{Obj}'s World Area";
	}
}
