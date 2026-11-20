using System.Collections.Generic;
using System.Linq;
using System.Text.Json.Nodes;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's LOD Group, which shows one of its levels at a time: the first whose Until the view has not passed. Past
/// the last, it shows nothing when Cull is on, and otherwise keeps showing the last. Get it with Obj.LodGroup, null
/// when the Obj has none.
/// <code>
/// // A slow machine: each level ends sooner.
/// if (Obj.LodGroup is { } lods)
/// {
///     lods.Levels = [.. lods.Levels.Select(level =&gt; level with { Until = level.Until * 1.5f })];
/// }
/// </code>
/// </summary>
public sealed class LodGroup : WholeComponent
{
	internal LodGroup(Obj obj)
		: base(obj, NativeWhole.Whole, NativeWhole.LodGroup)
	{
	}

	/// <summary>What its levels are measured by.</summary>
	public LodMeasure Measure
	{
		get => Choice("measure", LodMeasure.ScreenSize);
		set => Change("measure", PostJson.Of(value));
	}

	/// <summary>Whether it shows nothing past its last level.</summary>
	public bool Cull
	{
		get => Switch("cull", true);
		set => Change("cull", value);
	}

	/// <summary>Its levels, nearest first. A copy: set it whole to change them.</summary>
	public IReadOnlyList<LodLevel> Levels
	{
		get
		{
			var levels = new List<LodLevel>();
			foreach (var level in Settings()["levels"] as JsonArray ?? [])
			{
				var objs = (level?["objs"] as JsonArray ?? []).Select(id => new Obj(id!.GetValue<ulong>())).ToList();
				levels.Add(new LodLevel(objs, level?["until"]?.GetValue<float>() ?? 0f));
			}
			return levels;
		}
		set
		{
			var levels = new JsonArray();
			foreach (var level in value)
			{
				var objs = new JsonArray([.. level.Objs.Select(obj => (JsonNode?)JsonValue.Create(obj.Id))]);
				levels.Add(new JsonObject { ["objs"] = objs, ["until"] = level.Until });
			}
			Change("levels", levels);
		}
	}

	internal static LodGroup? Of(Obj obj)
	{
		var group = new LodGroup(obj);
		return group.IsValid ? group : null;
	}

	public override string ToString()
	{
		return $"{Obj}'s LOD Group";
	}
}
