using System;
using System.Collections.Generic;
using System.Numerics;
using System.Text.Json.Nodes;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Prefab Brush: in the editor, with its Obj selected, it paints prefabs as Objs onto the ground under that
/// Obj. Scripts set it up for an editor tool of their own; at play it does nothing. Get it with Obj.PrefabBrush, or
/// give an Obj one with Obj.AddPrefabBrush.
/// </summary>
public sealed class PrefabBrush : WholeComponent
{
	internal PrefabBrush(Obj obj)
		: base(obj, NativeWhole.Whole, NativeWhole.PrefabBrush)
	{
	}

	/// <summary>The prefabs it paints, each as often as its weight.</summary>
	public IReadOnlyList<PlacingChoice> Prefabs
	{
		get
		{
			var prefabs = new List<PlacingChoice>();
			if (Settings()["prefabs"] is JsonArray read)
			{
				foreach (var node in read)
				{
					var prefab = node?["prefab"] is JsonValue guid && guid.TryGetValue<string>(out var key) ? Prefab.Load(key) : Prefab.None;
					var weight = node?["weight"] is JsonValue w && w.TryGetValue<int>(out var number) ? number : 1;
					prefabs.Add(new PlacingChoice(prefab, weight));
				}
			}
			return prefabs;
		}
		set
		{
			var prefabs = new JsonArray();
			foreach (var choice in value)
			{
				prefabs.Add(new JsonObject { ["prefab"] = ((IAsset<Prefab>)choice.Prefab).AssetGuid, ["weight"] = choice.Weight });
			}
			Change("prefabs", prefabs);
		}
	}

	/// <summary>Paint scatters where a stroke goes; Single places one where you click.</summary>
	public BrushMode Mode
	{
		get => Choice("mode", BrushMode.Paint);
		set => Change("mode", PostJson.Of(value));
	}

	/// <summary>The brush's radius, in metres.</summary>
	public float Radius
	{
		get => Number("radius", 6f);
		set => Change("radius", value);
	}

	/// <summary>How many it scatters each 100 square metres.</summary>
	public float Density
	{
		get => Number("density", 3f);
		set => Change("density", value);
	}

	/// <summary>Whether each is turned any way, or as Turn says.</summary>
	public bool RandomTurn
	{
		get => Switch("random_turn", true);
		set => Change("random_turn", value);
	}

	/// <summary>How far each is turned about up, in degrees.</summary>
	public float Turn
	{
		get => Number("turn", 0f);
		set => Change("turn", value);
	}

	/// <summary>The least and most each is scaled by.</summary>
	public Vector2 Scale
	{
		get => Two("scale", Vector2.One);
		set => Change("scale", PostJson.Of(value));
	}

	/// <summary>Whether each leans with the slope of the ground under it.</summary>
	public bool Lean
	{
		get => Switch("lean", false);
		set => Change("lean", value);
	}

	/// <summary>The steepest ground one is put on, in degrees.</summary>
	public float Steepest
	{
		get => Number("steepest", 40f);
		set => Change("steepest", value);
	}

	internal static PrefabBrush? Of(Obj obj)
	{
		var brush = new PrefabBrush(obj);
		return brush.IsValid ? brush : null;
	}

	public override string ToString()
	{
		return $"{Obj}'s Prefab Brush";
	}
}
