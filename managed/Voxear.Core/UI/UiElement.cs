using System;
using System.Collections.Generic;
using System.Text;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// One element of a UiDocument: found by name in its markup, or made in code, as Unity's UI Toolkit makes them.
/// <code>
/// var hud = UiDocument.Create();
/// var panel = hud.Root.Add(new Panel());
/// var score = panel.Add(new Label("0"));
/// panel.Add(new Button("Pause")).Clicked += Pause;
/// </code>
/// <para>
/// An element made with new lives in your code until it is added to one that is on screen; from then on it shows,
/// and what you set on it shows at once. Its text, classes, style, visibility and value are kept while it waits.
/// Classes are how it changes look, as in CSS: <c>health.AddClass("low")</c> with a sheet rule for <c>.low</c>.
/// </para>
/// </summary>
public unsafe class UiElement
{
	private const int ClassRemove = 0;
	private const int ClassAdd = 1;
	private const int ClassToggle = 2;
	private const int ClassHas = 3;

	private readonly int kind;
	private readonly List<UiElement> children = new List<UiElement>();
	private readonly HashSet<string> classes = new HashSet<string>();
	private Action? clicked;
	private Action? changed;
	private string text;
	private string? style;
	private bool visible = true;
	private float value;

	internal UiElement(int id)
	{
		Id = id;
		kind = -1;
		text = string.Empty;
	}

	/// <summary>An element of this kind, made in code, to be added to another; `text` as the kind reads it.</summary>
	internal UiElement(UiKind kind, string text)
	{
		Id = -1;
		this.kind = (int)kind;
		this.text = text;
	}

	/// <summary>Runs once each time it is clicked, at the start of the next frame.</summary>
	public event Action? Clicked
	{
		add
		{
			clicked += value;
			UiElements.Listen(this);
		}
		remove
		{
			clicked -= value;
		}
	}

	/// <summary>Runs once each time its value changes from the screen: typed in, ticked, moved or picked.</summary>
	public event Action? Changed
	{
		add
		{
			changed += value;
			UiElements.Listen(this);
		}
		remove
		{
			changed -= value;
		}
	}

	/// <summary>
	/// A label's or a button's text. Set to "@menu.play", it shows that key's text in the game's language, and follows
	/// the language when it changes (see Localization); set to plain text, it stops following one. "@@" starts a text
	/// with a plain "@".
	/// </summary>
	public string Text
	{
		get { return IsShown ? ReadText() : text; }
		set { WriteText(value); }
	}

	public bool Visible
	{
		get { return IsShown ? Native.Api->UiVisible(Id, -1) != 0 : visible; }
		set { Show(value); }
	}

	/// <summary>A slider's value, a toggle's (1 when ticked and 0 when not), or the option a dropdown has picked.</summary>
	public float Value
	{
		get { return IsShown ? Native.Api->UiValue(Id, 0, 0f) : value; }
		set { WriteValue(value); }
	}

	/// <summary>Whether a toggle is ticked.</summary>
	public bool Checked
	{
		get { return Value != 0f; }
		set { Value = value ? 1f : 0f; }
	}

	/// <summary>Whether it was clicked since the last frame, for code that looks each frame rather than listens.</summary>
	public bool WasClicked => IsShown && Native.Api->UiClicked(Id) != 0;

	/// <summary>Whether its value changed from the screen since the last frame.</summary>
	public bool WasChanged => IsShown && Native.Api->UiChanged(Id) != 0;

	/// <summary>The elements added to it in code, in order. Elements its markup made are not listed.</summary>
	public IReadOnlyList<UiElement> Children => children;

	/// <summary>The element it was added to in code, or null.</summary>
	public UiElement? Parent { get; private set; }

	/// <summary>Whether it is on screen: found in a document, or added to an element that is.</summary>
	public bool IsShown => Id >= 0 && Native.IsConnected;

	internal int Id { get; private set; }

	internal bool HasListeners => clicked != null || changed != null;

	/// <summary>Adds an element at its end and gives it back, so it can be kept: <c>var bar = root.Add(new Slider());</c>.</summary>
	public T Add<T>(T child) where T : UiElement
	{
		if (child.Parent != null || child.Id >= 0)
		{
			throw new InvalidOperationException("That element is in another already; Remove it from there first.");
		}

		child.Parent = this;
		children.Add(child);
		if (IsShown)
		{
			child.ShowIn(this);
		}
		return child;
	}

	/// <summary>Takes out an element added to it, with everything in it. It can be added again, anywhere.</summary>
	public bool Remove(UiElement child)
	{
		if (child.Parent != this)
		{
			return false;
		}

		children.Remove(child);
		child.Parent = null;
		child.TakeOff();
		return true;
	}

	/// <summary>Takes out every element added to it in code.</summary>
	public void Clear()
	{
		foreach (var child in children.ToArray())
		{
			Remove(child);
		}
	}

	public void AddClass(string name)
	{
		ChangeClass(name, ClassAdd);
	}

	public void RemoveClass(string name)
	{
		ChangeClass(name, ClassRemove);
	}

	public void ToggleClass(string name)
	{
		ChangeClass(name, ClassToggle);
	}

	public bool HasClass(string name)
	{
		return ChangeClass(name, ClassHas);
	}

	/// <summary>Declarations that win over its sheets, as a style attribute's: <c>bar.SetStyle("width: 40%")</c>.</summary>
	public void SetStyle(string declarations)
	{
		style = declarations;
		if (!IsShown)
		{
			return;
		}

		var bytes = Encoding.UTF8.GetBytes(declarations);
		fixed (byte* written = bytes)
		{
			Native.Api->UiStyle(Id, written, bytes.Length);
		}
	}

	internal void RaiseClicked()
	{
		clicked?.Invoke();
	}

	internal void RaiseChanged()
	{
		OnChanged();
		changed?.Invoke();
	}

	/// <summary>For a kind of element to raise its own event when its value changes, before Changed.</summary>
	private protected virtual void OnChanged()
	{
	}

	/// <summary>For a kind of element to send what it holds besides what every element does, once it is on screen.</summary>
	private protected virtual void OnShown()
	{
	}

	/// <summary>For a kind of element to let go of what it keeps for being on screen, as it is taken off.</summary>
	private protected virtual void OnTakenOff()
	{
	}

	/// <summary>What the engine makes it from: its text, or for some kinds, such as a slider's range, what else they need.</summary>
	private protected virtual string MadeFrom => text;

	/// <summary>Raises Clicked and Changed on it and every element added to it, for drawers, which draw outside the frame.</summary>
	internal void DispatchTree()
	{
		if (HasListeners)
		{
			UiElements.DispatchOne(this);
		}

		foreach (var child in children.ToArray())
		{
			child.DispatchTree();
		}
	}

	/// <summary>Makes it on screen at the end of `parent`, as it was kept, and then what was added to it.</summary>
	private void ShowIn(UiElement parent)
	{
		var bytes = Encoding.UTF8.GetBytes(MadeFrom);
		fixed (byte* written = bytes)
		{
			Id = Native.Api->UiCreate(parent.Id, kind, written, bytes.Length);
		}
		if (Id < 0)
		{
			return;
		}

		UiElements.Adopt(this);
		foreach (var name in classes)
		{
			SendClass(name, ClassAdd);
		}
		if (style != null)
		{
			SetStyle(style);
		}
		if (!visible)
		{
			Show(false);
		}
		if (value != 0f)
		{
			WriteValue(value);
		}
		foreach (var child in children)
		{
			child.ShowIn(this);
		}
		OnShown();
	}

	/// <summary>Takes it off the screen, keeping what it shows, so it can show again elsewhere.</summary>
	private void TakeOff()
	{
		if (!IsShown)
		{
			return;
		}

		(text, value, visible) = (ReadText(), Value, Visible);
		Native.Api->UiRemove(Id);
		Forget();
	}

	private void Forget()
	{
		UiElements.Drop(this);
		OnTakenOff();
		Id = -1;
		foreach (var child in children)
		{
			child.Forget();
		}
	}

	private void Show(bool shown)
	{
		visible = shown;
		if (IsShown)
		{
			Native.Api->UiVisible(Id, shown ? 1 : 0);
		}
	}

	private void WriteValue(float to)
	{
		value = to;
		if (IsShown)
		{
			Native.Api->UiValue(Id, 1, to);
		}
	}

	private bool ChangeClass(string name, int change)
	{
		var had = classes.Contains(name);
		var has = change switch
		{
			ClassAdd => true,
			ClassRemove => false,
			ClassToggle => !had,
			_ => had,
		};
		if (has)
		{
			classes.Add(name);
		}
		else
		{
			classes.Remove(name);
		}
		return IsShown ? SendClass(name, change) : has;
	}

	private bool SendClass(string name, int change)
	{
		var bytes = Encoding.UTF8.GetBytes(name);
		fixed (byte* written = bytes)
		{
			return Native.Api->UiClass(Id, written, bytes.Length, change) != 0;
		}
	}

	private string ReadText()
	{
		var length = Native.Api->UiText(Id, null, 0);
		if (length <= 0)
		{
			return string.Empty;
		}
		var buffer = new byte[length];
		fixed (byte* into = buffer)
		{
			Native.Api->UiText(Id, into, buffer.Length);
		}
		return Encoding.UTF8.GetString(buffer);
	}

	private void WriteText(string written)
	{
		text = written;
		if (!IsShown)
		{
			return;
		}

		var bytes = Encoding.UTF8.GetBytes(written);
		fixed (byte* into = bytes)
		{
			Native.Api->UiSetText(Id, into, bytes.Length);
		}
	}
}
