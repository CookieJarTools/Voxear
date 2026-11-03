using System;

namespace Voxear;

/// <summary>A box to tick, with its label.</summary>
public class Toggle : UiElement
{
	private Action<bool>? toggled;

	public Toggle(string label = "", bool on = false) : base(UiKind.Toggle, label)
	{
		Checked = on;
	}

	/// <summary>A box labelled with a key's text, in the game's language.</summary>
	public Toggle(LocalizedString label, bool on = false) : base(UiKind.Toggle, label)
	{
		Checked = on;
	}

	/// <summary>Runs with whether it is ticked, each time it is ticked or unticked on screen.</summary>
	public event Action<bool>? Toggled
	{
		add
		{
			toggled += value;
			Listen();
		}
		remove
		{
			toggled -= value;
		}
	}

	/// <summary>Runs `handler` each time it is ticked or unticked on screen, and gives the toggle back.</summary>
	public Toggle OnToggled(Action<bool> handler)
	{
		Toggled += handler;
		return this;
	}

	/// <summary>Ticks it or not as `on` says, now and each time it is drawn again: <c>snap.Bind(() => Value.Snap)</c>.</summary>
	public Toggle Bind(Func<bool> on)
	{
		BindValue(() => on() ? 1f : 0f);
		return this;
	}

	private protected override bool KindListens => toggled != null;

	private protected override void RaiseKindChanged()
	{
		toggled?.Invoke(Checked);
	}
}
