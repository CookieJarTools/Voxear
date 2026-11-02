using System;
using System.Text;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The keyboard and mouse, or a gamepad, drawn as its keys, each bound one lit and labelled with its actions, as a
/// game's controls screen shows them. It follows the game's input on its own, rebinding included, every frame.
/// <code>
/// var map = panel.Add(new KeymapView(ControlScheme.Gamepad) { Colors = KeymapColors.Categories });
/// map.KeyClicked += path => Log.Info($"{path}: {Input.ControlUses(path).Count} actions");
/// </code>
/// <para>
/// Sheets style it by its classes: <c>keymap-key</c> for each key, with <c>assigned</c>, <c>hidden</c> or
/// <c>unused</c>, and <c>round</c>, <c>stick</c> or <c>pill</c> for its shape; <c>keymap-callout</c> for each label
/// round the device, whose border colour its line takes. Categories' colours come from the input actions. For a
/// screen of your own drawn another way, Input.ControlUses and Input.Categories give the same facts.
/// </para>
/// </summary>
public sealed unsafe class KeymapView : UiElement
{
	private ControlScheme scheme;
	private KeymapColors colors;
	private bool showHidden;
	private string? map;

	public KeymapView(ControlScheme scheme = ControlScheme.KeyboardMouse) : base(UiKind.Keymap, string.Empty)
	{
		this.scheme = scheme;
		Clicked += RaiseKeyClicked;
	}

	private KeymapView(int id) : base(id)
	{
		Clicked += RaiseKeyClicked;
	}

	/// <summary>Runs with the path of the control clicked, such as "Keyboard/W", at the start of the next frame.</summary>
	public event Action<string>? KeyClicked;

	/// <summary>Which device it draws: the keyboard and mouse, or a gamepad.</summary>
	public ControlScheme Scheme
	{
		get { return scheme; }
		set
		{
			scheme = value;
			Send();
		}
	}

	/// <summary>How its keys are coloured: by being bound, or by their actions' categories.</summary>
	public KeymapColors Colors
	{
		get { return colors; }
		set
		{
			colors = value;
			Send();
		}
	}

	/// <summary>Whether actions marked hidden, as a debug key, are labelled too.</summary>
	public bool ShowHidden
	{
		get { return showHidden; }
		set
		{
			showHidden = value;
			Send();
		}
	}

	/// <summary>Only this map's actions, by its name; null for every map's.</summary>
	public string? Map
	{
		get { return map; }
		set
		{
			map = value;
			Send();
		}
	}

	/// <summary>The key map named in a document's markup, as <c>&lt;Keymap name="controls" /&gt;</c>, or null.</summary>
	public static KeymapView? Find(UiDocument document, string name)
	{
		var found = document.Find(name);
		if (found == null)
		{
			return null;
		}
		if (found is KeymapView view)
		{
			return view;
		}
		var made = new KeymapView(found.Id);
		UiElements.Adopt(made);
		made.Send();
		return made;
	}

	private protected override void OnShown()
	{
		Send();
	}

	private void Send()
	{
		if (!IsShown)
		{
			return;
		}

		var bytes = Encoding.UTF8.GetBytes(map ?? string.Empty);
		fixed (byte* named = bytes)
		{
			Native.Api->UiKeymap(Id, (int)scheme, (int)colors, showHidden ? 1 : 0, named, bytes.Length);
		}
	}

	private void RaiseKeyClicked()
	{
		var path = ClickedPath();
		if (path.Length > 0)
		{
			KeyClicked?.Invoke(path);
		}
	}

	private string ClickedPath()
	{
		if (!IsShown)
		{
			return string.Empty;
		}

		var length = Native.Api->UiKeymapClicked(Id, null, 0);
		if (length <= 0)
		{
			return string.Empty;
		}
		var buffer = new byte[length];
		fixed (byte* into = buffer)
		{
			Native.Api->UiKeymapClicked(Id, into, buffer.Length);
		}
		return Encoding.UTF8.GetString(buffer);
	}
}
