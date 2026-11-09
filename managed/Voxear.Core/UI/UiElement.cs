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
/// panel.Add(new Button("Pause", Pause));
/// </code>
/// <para>
/// An element made with new lives in your code until it is added to one that is on screen; from then on it shows,
/// and what you set on it shows at once. Its text, classes, style, visibility and value are kept while it waits.
/// Classes are how it changes look, as in CSS: <c>health.AddClass("low")</c> with a sheet rule for <c>.low</c>.
/// </para>
/// <para>
/// What it shows can also come from code that runs again by itself: <c>score.Bind(() => $"{Score}")</c> works its
/// text out again each frame, or each time a drawer draws, and <c>title.Localize(Strings.Menu.Title)</c> shows a
/// key's text in the game's language, following it when it changes. See UiElementExtensions.
/// </para>
/// </summary>
public unsafe class UiElement
{
	private const int ClassRemove = 0;
	private const int ClassAdd = 1;
	private const int ClassToggle = 2;
	private const int ClassHas = 3;
	private const int StringTooltip = 0;

	private readonly int kind;
	private readonly List<UiElement> children = new List<UiElement>();
	private readonly HashSet<string> classes = new HashSet<string>();
	private Action? clicked;
	private Action? changed;
	private string text;
	private string? styleText;
	private UiStyle? style;
	private bool visible = true;
	private float value;
	private UiText tooltip;
	private Func<string>? textFrom;
	private bool textEachPass;
	private Func<float>? valueFrom;
	private ulong seenVersion;
	private UiHost? host;
	private (bool Changed, bool Clicked) pretended;

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

	/// <summary>An element of this kind showing a key's text in the game's language, following it as it changes.</summary>
	internal UiElement(UiKind kind, LocalizedString key) : this(kind, string.Empty)
	{
		Follow(new UiText(key));
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
	/// with a plain "@". Setting it also stops a Bind or Localize, so the text stays as set.
	/// </summary>
	public string Text
	{
		get { return IsShown ? ReadText() : text; }
		set
		{
			textFrom = null;
			WriteText(value);
		}
	}

	/// <summary>
	/// What shows when the pointer rests on it: written out, as "Fills the bar", or a key's text, as
	/// Strings.Hints.Fill, which follows the game's language.
	/// </summary>
	public UiText Tooltip
	{
		get { return tooltip; }
		set
		{
			tooltip = value;
			SendString(StringTooltip, value.ToString());
			Track();
		}
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

	internal bool HasListeners => clicked != null || changed != null || KindListens;

	/// <summary>Whether something of it comes from code to be worked out again: a Bind, or a key's text.</summary>
	private bool IsBound => textFrom != null || valueFrom != null || tooltip.IsLocalized || KindIsBound;

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

	/// <summary>
	/// Its own style, a property at a time, each winning over its sheets and keeping the rest:
	/// <c>bar.Style.Position = Position.Absolute;</c>, <c>bar.Style.Width = Length.Percent(50);</c>. SetStyles sets
	/// several at once.
	/// </summary>
	public UiStyle Style => style ??= new UiStyle(this);

	/// <summary>
	/// Its whole style as CSS text, as a style attribute's: <c>bar.SetStyle("width: 40%")</c>, in place of everything
	/// it had from code or its style attribute. For what Style does not have a property for yet.
	/// </summary>
	public void SetStyle(string declarations)
	{
		styleText = declarations;
		style?.Forget();
		SendStyleText(declarations);
	}

	/// <summary>Sends one property of its style, as Style sets it, or takes it out with an empty value.</summary>
	internal void SendStyleProperty(string name, string value)
	{
		if (!IsShown)
		{
			return;
		}

		var nameBytes = Encoding.UTF8.GetBytes(name);
		var valueBytes = Encoding.UTF8.GetBytes(value);
		fixed (byte* named = nameBytes)
		fixed (byte* written = valueBytes)
		{
			Native.Api->UiStyleProperty(Id, named, nameBytes.Length, written, valueBytes.Length);
		}
	}

	private void SendStyleText(string declarations)
	{
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

	/// <summary>Counts it as clicked at the next dispatch, as if on screen, for tests.</summary>
	internal void PretendClicked()
	{
		pretended.Clicked = true;
	}

	/// <summary>Gives it a value counted as changed on screen at the next dispatch, for tests.</summary>
	internal void PretendChanged(float to)
	{
		WriteValue(to);
		pretended.Changed = true;
	}

	/// <summary>Whether it was counted as changed and clicked since the last dispatch, which it forgets.</summary>
	internal (bool Changed, bool Clicked) TakePretended()
	{
		var taken = pretended;
		pretended = default;
		return taken;
	}

	internal void RaiseChanged()
	{
		RaiseKindChanged();
		changed?.Invoke();
	}

	/// <summary>
	/// Shows what `text` gives, now and again each time its host refreshes: each time when `eachPass`, or else when
	/// the game's language or text changes, as for a key's.
	/// </summary>
	internal void BindText(Func<string> text, bool eachPass)
	{
		(textFrom, textEachPass) = (text, eachPass);
		seenVersion = NativeText.Version();
		ShowBoundText(text());
		Track();
	}

	/// <summary>Shows a key's text, or text written out, as the element's text.</summary>
	internal void Follow(UiText shown)
	{
		if (!shown.IsLocalized)
		{
			Text = shown.ToString();
			return;
		}
		BindText(shown.ToString, eachPass: false);
	}

	/// <summary>Sets its value to what `to` gives, now and again each time its host refreshes.</summary>
	internal void BindValue(Func<float> to)
	{
		valueFrom = to;
		WriteValue(to());
		Track();
	}

	/// <summary>Works out what is bound again, and shows it where it changed. `version` counts the game's text.</summary>
	internal void RefreshBound(ulong version)
	{
		var languageChanged = version != seenVersion;
		seenVersion = version;
		if (textFrom != null && (textEachPass || languageChanged))
		{
			ShowBoundText(textFrom());
		}
		if (tooltip.IsLocalized && languageChanged)
		{
			SendString(StringTooltip, tooltip.ToString());
		}
		if (valueFrom != null)
		{
			var to = valueFrom();
			if (to != Value)
			{
				WriteValue(to);
			}
		}
		RefreshKind(languageChanged);
	}

	/// <summary>For a kind of element to raise its own event when its value changes, before Changed.</summary>
	private protected virtual void RaiseKindChanged()
	{
	}

	/// <summary>Whether a kind of element has handlers of its own events, as a toggle's Toggled.</summary>
	private protected virtual bool KindListens => false;

	/// <summary>Hears of its clicks and changes from now on, for a kind of element's own events.</summary>
	private protected void Listen()
	{
		UiElements.Listen(this);
	}

	/// <summary>Whether a kind of element holds text of its own to keep up to date, as a key's placeholder.</summary>
	private protected virtual bool KindIsBound => false;

	/// <summary>For a kind of element to show its own text again, as when the game's language changed.</summary>
	private protected virtual void RefreshKind(bool languageChanged)
	{
	}

	/// <summary>
	/// Joins the host keeping bound elements up to date, once it is bound: a drawer's or window's at once, the
	/// game's once it shows, so one made and never shown is not kept; see UiHost.
	/// </summary>
	private protected void Track()
	{
		var current = UiHost.Current;
		if (host != null || !IsBound || (!IsShown && current == UiHost.Game))
		{
			return;
		}

		host = current;
		host.Track(this);
	}

	/// <summary>
	/// Sends one of the texts an element holds besides its own: its tooltip (0), a text field's placeholder (1) or a
	/// dropdown's options (2), one a line.
	/// </summary>
	private protected void SendString(int which, string sent)
	{
		if (!IsShown)
		{
			return;
		}

		var bytes = Encoding.UTF8.GetBytes(sent);
		fixed (byte* written = bytes)
		{
			Native.Api->UiSetString(Id, which, written, bytes.Length);
		}
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
		if (styleText != null)
		{
			SendStyleText(styleText);
		}
		style?.SendAll();
		if (!visible)
		{
			Show(false);
		}
		if (value != 0f)
		{
			WriteValue(value);
		}
		if (!tooltip.IsEmpty)
		{
			SendString(StringTooltip, tooltip.ToString());
		}
		// Made with its text as written, so a key's "@menu.play" is given again to be read as one.
		if (textFrom == null && text.StartsWith('@'))
		{
			WriteText(text);
		}
		foreach (var child in children)
		{
			child.ShowIn(this);
		}
		OnShown();
		Track();
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
		host?.Untrack(this);
		host = null;
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

	/// <summary>Shows text code worked out, as it is: one starting with "@" is not read as a key.</summary>
	private void ShowBoundText(string shown)
	{
		if (shown == text)
		{
			return;
		}

		text = shown;
		if (IsShown)
		{
			var bytes = Encoding.UTF8.GetBytes(shown.StartsWith('@') ? "@" + shown : shown);
			fixed (byte* into = bytes)
			{
				Native.Api->UiSetText(Id, into, bytes.Length);
			}
		}
	}
}
