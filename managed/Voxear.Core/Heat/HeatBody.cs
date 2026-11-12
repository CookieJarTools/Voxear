using System;
using System.Globalization;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Heat Body: takes the temperature of where it is, and changes once it has been hot or cold enough for long
/// enough. Raw meat on a fire becomes seared meat; left in the snow, frozen meat. A character feels it as Freezing,
/// Cold, Comfortable, Hot or Scorching, by its comfort widened by its resistances. Get it with Obj.HeatBody.
/// <code>
/// protected override void Start()
/// {
///     HeatBody.Changed += change =&gt; Log.Info($"{change.Obj} is {change.Name}");
///     HeatBody.FeelsChanged += (body, feels) =&gt;
///     {
///         if (body.Obj == Obj &amp;&amp; feels == Feels.Freezing) Log.Info("Find a fire");
///     };
/// }
///
/// protected override void Update()
/// {
///     // A fur coat keeps out twenty degrees of cold; the game hurts what is past its band.
///     Obj.HeatBody.ColdResistance = wearingFur ? 20f : 0f;
///     health -= MathF.Abs(Obj.HeatBody.Beyond) * 0.1f * Time.Delta;
/// }
/// </code>
/// </summary>
public readonly struct HeatBody : IEquatable<HeatBody>, IObjComponent
{
	private const int TemperatureValue = 3;
	private const int FeelsValue = 4;
	private const int ColdValue = 5;
	private const int HeatValue = 6;
	private const int WarmsValue = 7;
	private const int ComfortValue = 8;
	private const int BeyondValue = 9;
	private const int PlaceValue = 10;
	private const int ProgressValue = 11;
	private const int ChangesValue = 12;
	private const int IndexOf = 1;
	private const int NameOf = 2;

	public static readonly HeatBody None = default;

	internal HeatBody(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>Raised when one of any Heat Body's changes happens, after the frame, with the change's name.</summary>
	public static event Action<HeatChange>? Changed;

	/// <summary>Raised when a character's Heat Body comes to feel otherwise: Cold to Freezing, Hot to Comfortable.</summary>
	public static event Action<HeatBody, Feels>? FeelsChanged;

	/// <summary>The Obj it belongs to.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Heat Body, or is gone.</summary>
	public bool IsValid => Obj.IsValid && Read(NativeComponent.Has, 0f) != 0f;

	/// <summary>Its temperature, in °C. Set it to start it off hot or cold: it moves on from there.</summary>
	public float Temperature
	{
		get { return Read(TemperatureValue, float.NaN); }
		set { Write(TemperatureValue, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>The temperature where it is, in °C, as it last looked: what its temperature moves towards.</summary>
	public float PlaceTemperature => Read(PlaceValue, float.NaN);

	/// <summary>How it feels its temperature.</summary>
	public Feels Feels => (Feels)(int)Read(FeelsValue, (float)Feels.Comfortable);

	/// <summary>How many °C past its band it is: above 0 hotter than it, below 0 colder, 0 within it.</summary>
	public float Beyond => Read(BeyondValue, 0f);

	/// <summary>How many °C colder than its comfort it is still comfortable at, as warm clothes give.</summary>
	public float ColdResistance
	{
		get { return Read(ColdValue, 0f); }
		set { Write(ColdValue, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>How many °C hotter than its comfort it is still comfortable at, as a cooling drink gives.</summary>
	public float HeatResistance
	{
		get { return Read(HeatValue, 0f); }
		set { Write(HeatValue, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>How many seconds it takes to come about two thirds of the way to where it is; 0 at once.</summary>
	public float WarmsIn
	{
		get { return Read(WarmsValue, 0f); }
		set { Write(WarmsValue, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>The temperatures it is comfortable between, in °C, before its resistances.</summary>
	public (float From, float To) Comfort
	{
		get { return NativeComponent.Get(NativeComponent.HeatBody, Obj, ComfortValue, out var value) ? (value.X, value.Y) : (0f, 0f); }
		set { Write(ComfortValue, new Vector4(value.From, value.To, 0f, 0f)); }
	}

	/// <summary>How many changes its list has.</summary>
	public int ChangeCount => (int)Read(ChangesValue, 0f);

	/// <summary>How far its change called <paramref name="change"/> has counted; nothing done for one it has not.</summary>
	public HeatProgress Progress(string change)
	{
		var index = NativeComponent.Ask(NativeComponent.HeatBody, Obj, IndexOf, change);
		return int.TryParse(index, NumberStyles.Integer, CultureInfo.InvariantCulture, out var at) ? Progress(at) : default;
	}

	/// <summary>How far its change at <paramref name="index"/> in its list has counted.</summary>
	public HeatProgress Progress(int index)
	{
		var asked = new Vector4(index, 0f, 0f, 0f);
		if (index < 0 || !NativeComponent.Set(NativeComponent.HeatBody, Obj, ProgressValue, asked))
		{
			return default;
		}
		NativeComponent.Get(NativeComponent.HeatBody, Obj, ProgressValue, out var value);
		return new HeatProgress(value.X, value.Y, value.Z, value.W != 0f);
	}

	/// <summary>The name of its change at <paramref name="index"/> in its list; empty for none.</summary>
	public string ChangeName(int index)
	{
		return NativeComponent.Ask(NativeComponent.HeatBody, Obj, NameOf, index.ToString(CultureInfo.InvariantCulture)) ?? string.Empty;
	}

	/// <summary>Takes the Heat Body off its Obj.</summary>
	public void Remove()
	{
		Write(NativeComponent.Remove, Vector4.Zero);
	}

	internal static void Raise(HeatChange change)
	{
		try
		{
			Changed?.Invoke(change);
		}
		catch (Exception exception)
		{
			Log.Exception(exception);
		}
	}

	internal static void Raise(HeatBody body, Feels feels)
	{
		try
		{
			FeelsChanged?.Invoke(body, feels);
		}
		catch (Exception exception)
		{
			Log.Exception(exception);
		}
	}

	private float Read(int what, float otherwise)
	{
		return NativeComponent.Number(NativeComponent.HeatBody, Obj, what, otherwise);
	}

	private void Write(int what, Vector4 value)
	{
		NativeComponent.Set(NativeComponent.HeatBody, Obj, what, value);
	}

	public static bool operator ==(HeatBody left, HeatBody right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(HeatBody left, HeatBody right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(HeatBody other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is HeatBody same && Equals(same);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Heat Body";
	}
}
