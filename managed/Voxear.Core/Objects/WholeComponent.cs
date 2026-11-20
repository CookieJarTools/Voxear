using System;
using System.Numerics;
using System.Text.Json.Nodes;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's component that scripts read and write a setting at a time, as its scene file writes it. A setting set to
/// what it cannot be is reported in the Console, and nothing changes.
/// </summary>
public abstract class WholeComponent : IObjComponent
{
	private protected WholeComponent(Obj obj, int kind)
	{
		Obj = obj;
		KindNumber = kind;
		Which = -1;
	}

	/// <summary>One of the components read whole through the one kind for them, NativeWhole.Whole.</summary>
	private protected WholeComponent(Obj obj, int kind, int which)
	{
		Obj = obj;
		KindNumber = kind;
		Which = which;
	}

	/// <summary>The Obj whose component it is.</summary>
	public Obj Obj { get; }

	/// <summary>False once its Obj has none, or is gone.</summary>
	public bool IsValid => Obj.IsValid && (Which < 0
		? NativeComponent.Number(KindNumber, Obj, NativeComponent.Has, 0f) != 0f
		: NativeWhole.HasWhole(Which, Obj));

	private protected int KindNumber { get; }

	private protected int Which { get; }

	/// <summary>Takes the component off its Obj.</summary>
	public void Remove()
	{
		if (Which >= 0)
		{
			NativeWhole.AddOrRemoveWhole(Which, Obj, false);
			return;
		}
		NativeComponent.Set(KindNumber, Obj, NativeComponent.Remove, Vector4.Zero);
	}

	public override bool Equals(object? other)
	{
		return other is WholeComponent component && component.GetType() == GetType() && component.Obj == Obj;
	}

	public override int GetHashCode()
	{
		return HashCode.Combine(GetType(), Obj);
	}

	private protected JsonObject Settings()
	{
		return (Which < 0 ? NativeWhole.Read(KindNumber, Obj) : NativeWhole.ReadWhole(Which, Obj)) ?? [];
	}

	private protected float Number(string key, float otherwise)
	{
		var value = otherwise;
		PostJson.Read(Settings(), key, ref value);
		return value;
	}

	private protected bool Switch(string key, bool otherwise)
	{
		var value = otherwise;
		PostJson.Read(Settings(), key, ref value);
		return value;
	}

	private protected Color Colour(string key, Color otherwise)
	{
		var value = otherwise;
		PostJson.Read(Settings(), key, ref value);
		return value;
	}

	private protected Vector2 Two(string key, Vector2 otherwise)
	{
		return Settings()[key] is JsonArray { Count: 2 } read ? new Vector2(read[0]!.GetValue<float>(), read[1]!.GetValue<float>()) : otherwise;
	}

	private protected Vector3 Three(string key, Vector3 otherwise)
	{
		var value = otherwise;
		PostJson.Read(Settings(), key, ref value);
		return value;
	}

	private protected T Choice<T>(string key, T otherwise) where T : struct, Enum
	{
		var value = otherwise;
		PostJson.ReadChoice(Settings(), key, ref value);
		return value;
	}

	private protected void Change(string key, JsonNode? value)
	{
		var fields = new JsonObject { [key] = value };
		if (Which >= 0)
		{
			if (!NativeWhole.ChangeWhole(Which, Obj, fields) && Native.IsConnected && !IsValid)
			{
				Log.Error($"{this}: {Obj} has none.");
			}
			return;
		}
		NativeWhole.Change(KindNumber, Obj, fields, ToString() ?? string.Empty);
	}
}
