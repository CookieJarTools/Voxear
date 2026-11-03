namespace Voxear;

/// <summary>
/// An action's controls drawn as glyphs, as games show "press A to jump": a keycap for a key, the mouse with its button
/// filled, and a gamepad's buttons as the player's own pad marks and colours them, an Xbox pad's lettered A, a
/// PlayStation pad's cross. It follows the device the player used last, and rebinding, by itself every frame.
/// <code>
/// var row = panel.Add(new Panel("hint"));
/// row.Add(new PromptView("Jump"));
/// row.Add(new Label("Jump"));
/// </code>
/// <para>
/// In a UI document, write <c>&lt;Prompt action="Jump" /&gt;</c>. Sheets style it by its classes: <c>prompt</c>, and
/// <c>prompt-glyph</c> for each control with <c>key</c>, <c>round</c>, <c>pill</c>, <c>square</c>, <c>stick</c> or
/// <c>bare</c> for its shape, holding a <c>prompt-text</c> label or a <c>prompt-icon</c>.
/// </para>
/// </summary>
public sealed class PromptView : UiElement
{
	private string action;

	public PromptView(string action) : base(UiKind.Prompt, action)
	{
		this.action = action;
	}

	/// <summary>The action whose controls it shows, by name, as "Jump" or "Player/Jump".</summary>
	public string Action
	{
		get { return action; }
		set
		{
			action = value;
			Text = value;
		}
	}
}
