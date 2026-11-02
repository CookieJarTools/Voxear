using System;

namespace Voxear;

/// <summary>A box to tick, with its label.</summary>
public class Toggle : UiElement
{
	public Toggle(string label = "", bool on = false) : base(UiKind.Toggle, label)
	{
		Checked = on;
	}

	/// <summary>Runs with whether it is ticked, each time it is ticked or unticked on screen.</summary>
	public event Action<bool>? Toggled;

	private protected override void OnChanged()
	{
		Toggled?.Invoke(Checked);
	}
}
