using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Screen UI: a .vui document shown over the game's view for as long as the Obj is there, as a HUD or a
/// menu, with no script to load it. Its document is picked in the Inspector, and its Document is the UiDocument it
/// shows, ready from Awake on. Switching the Obj off hides it and keeps its handlers; destroying the Obj closes it.
/// Behaviours on the Obj get OnUiBuilt with its document. Get it with Obj.ScreenUi, or give an Obj one with
/// Obj.AddScreenUi.
/// <code>
/// protected override void OnUiBuilt(UiDocument ui)
/// {
/// 	var coins = ui.Find("coins")!;
/// 	ui.Find("shop")!.Clicked += OpenShop;
/// }
/// </code>
/// </summary>
public readonly struct ScreenUi : IEquatable<ScreenUi>, IObjComponent
{
	private const int OrderValue = 3;
	private const int DocumentValue = 4;

	public static readonly ScreenUi None = default;

	internal ScreenUi(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj it belongs to.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Screen UI, or is gone.</summary>
	public bool IsValid => Obj.IsValid && NativeComponent.Number(NativeComponent.ScreenUi, Obj, NativeComponent.Has, 0f) != 0f;

	/// <summary>
	/// The document it shows, loaded the first time a script asks, so it is there in Awake, OnEnable and Start. Null
	/// when it has no document, or its file cannot be read, which the Console says why, and away from a running game.
	/// </summary>
	public UiDocument? Document
	{
		get
		{
			if (!NativeComponent.Get(NativeComponent.ScreenUi, Obj, DocumentValue, out var value) || value.Y == 0f)
			{
				return null;
			}
			return new UiDocument((int)BitConverter.SingleToUInt32Bits(value.X));
		}
	}

	/// <summary>Which Screen UI shows over which: a higher order over a lower.</summary>
	public int Order
	{
		get { return (int)NativeComponent.Number(NativeComponent.ScreenUi, Obj, OrderValue, 0f); }
		set { NativeComponent.Set(NativeComponent.ScreenUi, Obj, OrderValue, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>Takes the Screen UI off its Obj, and its document off the screen.</summary>
	public void Remove()
	{
		NativeComponent.Set(NativeComponent.ScreenUi, Obj, NativeComponent.Remove, Vector4.Zero);
	}

	public static bool operator ==(ScreenUi left, ScreenUi right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(ScreenUi left, ScreenUi right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(ScreenUi other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is ScreenUi screen && Equals(screen);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Screen UI";
	}
}
