using System;
using System.Collections.Generic;
using System.Linq;

namespace Voxear;

/// <summary>A choice of options, one picked at a time; Selected is the one picked, by its place.</summary>
public class Dropdown : UiElement
{
	private const int StringOptions = 2;

	private readonly UiText[] options;
	private readonly bool localized;
	private string[] shown;
	private Action<int>? picked;

	public Dropdown(params string[] options) : base(UiKind.Dropdown, string.Empty)
	{
		this.options = Array.ConvertAll(options, option => new UiText(option));
		shown = options;
	}

	/// <summary>
	/// Options that are keys' texts, or written out, each in the game's language and following it:
	/// <c>new Dropdown(Strings.Difficulty.Easy, Strings.Difficulty.Hard)</c>.
	/// </summary>
	public Dropdown(params UiText[] options) : base(UiKind.Dropdown, string.Empty)
	{
		this.options = options;
		localized = options.Any(option => option.IsLocalized);
		shown = Array.ConvertAll(options, option => option.ToString());
	}

	/// <summary>Runs with the option picked, by its place, each time one is picked on screen.</summary>
	public event Action<int>? Picked
	{
		add
		{
			picked += value;
			Listen();
		}
		remove
		{
			picked -= value;
		}
	}

	/// <summary>The options as they show now, in the game's language.</summary>
	public IReadOnlyList<string> Options => shown;

	/// <summary>The option picked, by its place in Options.</summary>
	public int Selected
	{
		get { return (int)Value; }
		set { Value = value; }
	}

	/// <summary>Runs `handler` with the option picked, by its place, each time one is picked, and gives the dropdown back.</summary>
	public Dropdown OnPicked(Action<int> handler)
	{
		Picked += handler;
		return this;
	}

	/// <summary>Picks the option `selected` says, by its place, now and each time it is drawn again.</summary>
	public Dropdown Bind(Func<int> selected)
	{
		BindValue(() => selected());
		return this;
	}

	private protected override string MadeFrom => string.Join('\n', shown);

	private protected override bool KindIsBound => localized;

	private protected override void RefreshKind(bool languageChanged)
	{
		if (!languageChanged || !localized)
		{
			return;
		}

		shown = Array.ConvertAll(options, option => option.ToString());
		SendString(StringOptions, string.Join('\n', shown));
	}

	private protected override bool KindListens => picked != null;

	private protected override void RaiseKindChanged()
	{
		picked?.Invoke(Selected);
	}
}
