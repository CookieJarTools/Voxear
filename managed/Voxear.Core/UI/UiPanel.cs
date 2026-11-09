using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's UI Panel: a .vui document shown on a rectangle in the world, centred on the Obj, which players read and
/// click by pointing at it. It reads the right way round looking the way its Obj faces. Its Document is the
/// UiDocument it shows, so its elements are found, read and clicked as a screen document's are. Get it with
/// Obj.UiPanel, or give an Obj one with Obj.AddUiPanel; its document is picked in the Inspector.
/// <code>
/// // A terminal by a door: its button opens the door.
/// var screen = terminal.UiPanel.Document;
/// screen?.Find("open")!.Clicked += door.Open;
/// </code>
/// </summary>
public readonly struct UiPanel : IEquatable<UiPanel>, IObjComponent
{
	private const int SizeValue = 3;
	private const int PixelsPerMetreValue = 4;
	private const int FaceCameraValue = 5;
	private const int ClickableValue = 6;
	private const int DocumentValue = 7;

	public static readonly UiPanel None = default;

	internal UiPanel(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj it is shown on.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no UI Panel, or is gone.</summary>
	public bool IsValid => Obj.IsValid && Number(NativeComponent.Has) != 0f;

	/// <summary>The document it shows, null while it shows none or has not loaded it yet.</summary>
	public UiDocument? Document
	{
		get
		{
			if (!NativeComponent.Get(NativeComponent.UiPanel, Obj, DocumentValue, out var value) || value.Y == 0f)
			{
				return null;
			}
			return new UiDocument((int)BitConverter.SingleToUInt32Bits(value.X));
		}
	}

	/// <summary>How wide and high it is, in metres, before its Obj's scale.</summary>
	public Vector2 Size
	{
		get
		{
			return NativeComponent.Get(NativeComponent.UiPanel, Obj, SizeValue, out var value)
				? new Vector2(value.X, value.Y)
				: Vector2.Zero;
		}
		set { NativeComponent.Set(NativeComponent.UiPanel, Obj, SizeValue, new Vector4(value.X, value.Y, 0f, 0f)); }
	}

	/// <summary>How many of the UI's pixels a metre holds: a panel 1.6 m wide at 500 lays its UI out 800 pixels wide.</summary>
	public float PixelsPerMetre
	{
		get { return Number(PixelsPerMetreValue); }
		set { Set(PixelsPerMetreValue, value); }
	}

	/// <summary>Whether it turns to face the camera, keeping its up, rather than its Obj's way.</summary>
	public bool FaceCamera
	{
		get { return Number(FaceCameraValue) != 0f; }
		set { Set(FaceCameraValue, value ? 1f : 0f); }
	}

	/// <summary>Whether the pointer works it. Off, clicks pass through to the game.</summary>
	public bool Clickable
	{
		get { return Number(ClickableValue) != 0f; }
		set { Set(ClickableValue, value ? 1f : 0f); }
	}

	/// <summary>Takes the UI Panel off its Obj.</summary>
	public void Remove()
	{
		NativeComponent.Set(NativeComponent.UiPanel, Obj, NativeComponent.Remove, Vector4.Zero);
	}

	public static bool operator ==(UiPanel left, UiPanel right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(UiPanel left, UiPanel right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(UiPanel other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is UiPanel panel && Equals(panel);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s UI Panel";
	}

	private float Number(int what)
	{
		return NativeComponent.Number(NativeComponent.UiPanel, Obj, what, 0f);
	}

	private void Set(int what, float value)
	{
		NativeComponent.Set(NativeComponent.UiPanel, Obj, what, new Vector4(value, 0f, 0f, 0f));
	}
}
