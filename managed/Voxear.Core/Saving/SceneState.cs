using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using System.Threading.Tasks;
using Voxear.Interop;
using Voxear.Saving;
using Voxear.Scripting;

namespace Voxear;

/// <summary>
/// The open scenes as the player left them: the values behaviours marked [Saved], the Objs the scenes loaded that
/// have been destroyed since, and the Objs scripts made from prefabs, with where each one is. Capture takes it now,
/// and it saves like any value, so it can be one part of a SaveFile. LoadAsync loads its level again and puts it
/// back; Save.WriteScene and Save.LoadScene do both in one call.
/// </summary>
public sealed class SceneState
{
	private const string Context = "the scene";

	/// <summary>Frames until IsLoading turns false, once a state is put back: the rest of this one, and none after.</summary>
	private static int framesLeft;

	internal SceneState(SaveNode root)
	{
		Root = root;
	}

	internal SaveNode Root { get; }

	/// <summary>Whether a save of the scene is being loaded, from the start of LoadAsync to the end of the frame it is put back in. See Save.IsLoadingScene.</summary>
	internal static bool IsLoading { get; private set; }

	/// <summary>Takes the state of the open scenes now.</summary>
	public static SceneState Capture()
	{
		var (level, gone, copies) = Describe();
		var numbers = new Dictionary<ulong, int>();
		for (var number = 0; number < copies.Count; number++)
		{
			numbers[copies[number].Root] = number;
		}

		var root = SaveNode.NewTable();
		using (SpawnedKeys.Writing(numbers))
		using (SaveWriter.Begin(Context))
		{
			root["level"] = level;
			root["gone"] = SaveNode.From(gone);
			root["spawned"] = SaveNode.NewList(copies.Select(Copy));
			root["saved"] = SaveNode.NewList(BehaviourTable.Live().Select(Values).OfType<SaveNode>());
		}
		return new SceneState(root);
	}

	/// <summary>
	/// Puts the state back into the open scenes, as they are after their level has loaded afresh: the Objs that were
	/// destroyed are destroyed, the Objs scripts made since the level loaded give way to the ones the state made,
	/// which are made again, and every [Saved] value is set. LoadAsync loads the level first, which is what a
	/// Continue button wants.
	/// </summary>
	public void Apply()
	{
		IsLoading = true;
		framesLeft = 2;
		foreach (var copy in Describe().Copies)
		{
			new Obj(copy.Root).Destroy();
		}
		foreach (var key in Root["gone"].Items)
		{
			var obj = NativeSaves.FindObj(key.AsString() ?? string.Empty);
			if (obj.IsValid)
			{
				obj.Destroy();
			}
		}

		var made = MakeCopies();
		using (SpawnedKeys.Reading(made))
		using (SaveReader.Begin(Context))
		{
			foreach (var values in Root["saved"].Items)
			{
				Restore(values);
			}
		}
	}

	/// <summary>Loads the level the state was taken in again, its scenes afresh, and then puts the state back.</summary>
	public async Task LoadAsync()
	{
		var level = Root["level"].AsString();
		IsLoading = true;
		framesLeft = 0;
		try
		{
			if (level != null)
			{
				await Level.LoadAsync(level, new LevelLoadOptions { ReloadShared = true });
			}
		}
		catch
		{
			IsLoading = false;
			throw;
		}
		Apply();
	}

	/// <summary>Once a frame: IsLoading lasts to the end of the frame a state was put back in.</summary>
	internal static void Update()
	{
		if (framesLeft > 0 && --framesLeft == 0)
		{
			IsLoading = false;
		}
	}

	/// <summary>What the engine says about the open scenes: the level's GUID, the keys of the Objs gone, and each copy scripts made.</summary>
	private static (string? Level, List<string> Gone, List<(ulong Root, string Prefab, string? Parent)> Copies) Describe()
	{
		var gone = new List<string>();
		var copies = new List<(ulong Root, string Prefab, string? Parent)>();
		var json = NativeSaves.SceneState();
		if (json == null)
		{
			return (null, gone, copies);
		}

		using var document = JsonDocument.Parse(json);
		var state = document.RootElement;
		gone.AddRange(state.GetProperty("gone").EnumerateArray().Select(key => key.GetString()!));
		foreach (var copy in state.GetProperty("spawned").EnumerateArray())
		{
			copies.Add((copy.GetProperty("root").GetUInt64(), copy.GetProperty("prefab").GetString()!, copy.GetProperty("parent").GetString()));
		}
		return (state.GetProperty("level").GetString(), gone, copies);
	}

	private static SaveNode Copy((ulong Root, string Prefab, string? Parent) copy)
	{
		var node = SaveNode.NewTable();
		node["prefab"] = copy.Prefab;
		node["parent"] = SpawnedKeys.ForSave(copy.Parent);
		node["place"] = SaveNode.From(new Obj(copy.Root).Transform);
		return node;
	}

	/// <summary>A behaviour's [Saved] values, with what finds it again: its Obj, its class, and which of that class on the Obj it is.</summary>
	private static SaveNode? Values(Behaviour behaviour)
	{
		var type = behaviour.GetType();
		var members = SceneMembers.Of(type);
		if (members.Count == 0)
		{
			return null;
		}

		var key = SpawnedKeys.ForSave(NativeSaves.Key(NativeSaves.KeyObj, behaviour.Obj.Id, string.Empty));
		if (key == null)
		{
			Log.Warning($"{type.Name} on {behaviour.Obj.Name} has [Saved] values, but its Obj is in no scene file and was not made from a prefab, so a save of the scene cannot find it again.");
			return null;
		}

		var node = SaveNode.NewTable();
		node["obj"] = key;
		node["type"] = SaveTypes.NameOf(type);
		node["index"] = Alike(behaviour.Obj, type).IndexOf(behaviour);
		node["values"] = SaveWriter.WriteMembers(behaviour, members, Context);
		return node;
	}

	/// <summary>The copies made again, by their number in the state, each under its parent, which is made first when it is a copy too.</summary>
	private List<Obj> MakeCopies()
	{
		var copies = Root["spawned"].Items;
		var made = Enumerable.Repeat(Obj.None, copies.Count).ToList();
		var done = new bool[copies.Count];
		for (var progress = true; progress;)
		{
			progress = false;
			for (var number = 0; number < copies.Count; number++)
			{
				var parent = copies[number]["parent"].AsString();
				var waiting = SpawnedKeys.Number(parent) is var under and >= 0 && under < copies.Count && !done[under];
				if (!done[number] && !waiting)
				{
					made[number] = MakeCopy(copies[number], parent, made);
					done[number] = progress = true;
				}
			}
		}
		return made;
	}

	private static Obj MakeCopy(SaveNode copy, string? parentKey, List<Obj> made)
	{
		var prefab = Prefab.Load(copy["prefab"].AsString() ?? string.Empty);
		if (!prefab.IsValid)
		{
			return Obj.None;
		}

		var key = parentKey == null ? null : parentKey.StartsWith(SpawnedKeys.Prefix, System.StringComparison.Ordinal) ? SpawnedKeys.Lookup(parentKey, made) : parentKey;
		var parent = key == null ? Obj.None : NativeSaves.FindObj(key);
		var obj = parent.IsValid ? prefab.Instantiate(parent) : prefab.Instantiate();
		if (obj.IsValid)
		{
			obj.Transform = copy["place"].As<Transform>();
		}
		return obj;
	}

	private static void Restore(SaveNode values)
	{
		var typeName = values["type"].AsString() ?? string.Empty;
		var key = SpawnedKeys.ForLookup(values["obj"].AsString() ?? string.Empty);
		var obj = key == null ? Obj.None : NativeSaves.FindObj(key);
		var type = SaveTypes.Find(typeName);
		if (!obj.IsValid || type == null)
		{
			Log.Warning($"A save of the scene has values for a {typeName} that is not here now, so they are left out.");
			return;
		}

		var behaviour = Alike(obj, type).ElementAtOrDefault(values["index"].AsInt());
		if (behaviour == null)
		{
			Log.Warning($"A save of the scene has values for a {type.Name} on {obj.Name}, which has none now, so they are left out.");
			return;
		}
		SaveReader.ReadMembers(values["values"], behaviour, SceneMembers.Of(type), Context);
	}

	/// <summary>The behaviours of exactly this class on an Obj, in its order.</summary>
	private static List<Behaviour> Alike(Obj obj, System.Type type)
	{
		return Native.Behaviours(obj.Id).Select(BehaviourTable.Get).OfType<Behaviour>().Where(behaviour => behaviour.GetType() == type).ToList();
	}
}
