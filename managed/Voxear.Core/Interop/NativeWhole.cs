using System.Text.Json.Nodes;

namespace Voxear.Interop;

/// <summary>Components read whole as JSON, laid out as their scene files write them, and changed a setting at a time.</summary>
internal static class NativeWhole
{
	public const int PostVolume = 42;
	public const int WindZone = 43;
	public const int Ocean = 44;
	public const int Pool = 45;
	public const int Stream = 46;

	/// <summary>
	/// Components with calls of their own that are also read and changed whole, through one kind: which, as the
	/// engine's script_components.rs numbers them.
	/// </summary>
	public const int Whole = 47;
	public const int BoneAttachment = 0;
	public const int ParticleSystem = 1;
	public const int NavArea = 2;
	public const int NavLink = 3;
	public const int Foliage = 4;
	public const int LodGroup = 5;
	public const int DryAreaMesh = 6;
	public const int LightCookie = 7;
	public const int LightProfile = 8;
	public const int GroundFit = 9;
	public const int StreamsWithGround = 10;
	public const int Road = 11;
	public const int AreaFill = 12;
	public const int Placement = 13;
	public const int PrefabBrush = 14;

	private const int ReadAll = 0;
	private const int ChangeSome = 1;

	/// <summary>Every setting of the Obj's component; null when it has none, or away from the engine.</summary>
	public static JsonObject? Read(int kind, Obj obj)
	{
		return NativeComponent.Text(kind, obj, ReadAll) is { } json ? JsonNode.Parse(json) as JsonObject : null;
	}

	/// <summary>Changes the settings given of the Obj's component; the Console says why where it cannot.</summary>
	public static void Change(int kind, Obj obj, JsonObject fields, string what)
	{
		var changed = NativeComponent.Tell(kind, obj, ChangeSome, fields.ToJsonString());
		if (!changed && Native.IsConnected && NativeComponent.Number(kind, obj, NativeComponent.Has, 0f) == 0f)
		{
			Log.Error($"{what}: {obj} has none.");
		}
	}

	/// <summary>Every setting of the Obj's component of a kind read whole; null when it has none.</summary>
	public static JsonObject? ReadWhole(int which, Obj obj)
	{
		return NativeComponent.Text(Whole, obj, which << 1 | ReadAll) is { } json ? JsonNode.Parse(json) as JsonObject : null;
	}

	/// <summary>Changes the settings given of the Obj's component of a kind read whole; false when it has none.</summary>
	public static bool ChangeWhole(int which, Obj obj, JsonObject fields)
	{
		return NativeComponent.Tell(Whole, obj, which << 1 | ChangeSome, fields.ToJsonString());
	}

	/// <summary>An asset of the Obj's that only the engine loads, as it names it; null with none.</summary>
	public static string? Asset(int which, Obj obj)
	{
		var named = NativeComponent.Text(Whole, obj, which << 1 | ReadAll);
		return string.IsNullOrEmpty(named) ? null : named;
	}

	/// <summary>Loads an asset of the Obj's in place of the one it has, or with an empty reference takes it off.</summary>
	public static bool SetAsset(int which, Obj obj, string reference)
	{
		return NativeComponent.Tell(Whole, obj, which << 1 | ChangeSome, reference);
	}

	/// <summary>Whether the Obj has a component of a kind read whole.</summary>
	public static bool HasWhole(int which, Obj obj)
	{
		return NativeComponent.Number(Whole, obj, which << 2 | NativeComponent.Has, 0f) != 0f;
	}

	/// <summary>Gives the Obj a component of a kind read whole, unless it has one, or takes it off.</summary>
	public static void AddOrRemoveWhole(int which, Obj obj, bool add)
	{
		NativeComponent.Set(Whole, obj, which << 2 | (add ? NativeComponent.Add : NativeComponent.Remove), System.Numerics.Vector4.Zero);
	}

	/// <summary>The level's Seasons or Climate read, with <paramref name="fields"/> changed first; null where it has none.</summary>
	public static JsonObject? Level(string component, JsonObject? fields = null)
	{
		var asked = new JsonObject { ["component"] = component };
		if (fields is not null)
		{
			asked["fields"] = fields;
		}
		var json = NativePost.Call(NativePost.LevelComponent, asked.ToJsonString());
		return string.IsNullOrEmpty(json) ? null : JsonNode.Parse(json) as JsonObject;
	}
}
