using System;
using System.Collections.Generic;
using System.Text;

namespace Voxear;

/// <summary>
/// A key's text in the game's language with its {0}, {1} and on filled in from code, laid out as a row of text and
/// controls. An action shows its controls as a prompt does, for the device the player used last, so switching from
/// the keyboard to a pad changes the icon by itself; a value from Bind is worked out again each frame; and the text
/// follows the language as it changes.
/// <code>
/// // shop.buy = "Press {0} to buy for {1, currency}"
/// var confirm = Input.Action("Confirm");
/// panel.Add(new LocalizedLabel(Strings.Shop.Buy, TextParam.Action(confirm), TextParam.Bind(() => price)));
/// </code>
/// <para>
/// Sheets style it by its classes: <c>localized-label</c> for the row, and <c>localized-label-text</c> for each
/// stretch of text between controls. The controls are prompts, styled as every prompt is.
/// </para>
/// </summary>
public sealed class LocalizedLabel : UiElement
{
	private const char ActionStart = '';
	private const char ActionEnd = '';

	private readonly List<TextParam> values;
	private readonly List<LabelPart> parts = [];
	private LocalizedString key;

	public LocalizedLabel(LocalizedString key, params TextParam[] values) : base(UiKind.Panel, string.Empty)
	{
		this.key = key;
		this.values = [.. values];
		AddClass("localized-label");
		this.SetStyles(flexDirection: FlexDirection.Row, flexWrap: FlexWrap.Wrap, alignItems: Align.Center, columnGap: 4);
		Refresh();
	}

	/// <summary>The key whose text it shows.</summary>
	public LocalizedString Key
	{
		get { return key; }
		set
		{
			key = value;
			Refresh();
		}
	}

	/// <summary>The values it fills in, {0} first.</summary>
	public IReadOnlyList<TextParam> Values => values;

	private protected override bool KindIsBound => true;

	/// <summary>Gives {index} another value, and shows the text again.</summary>
	public void Set(int index, TextParam value)
	{
		while (values.Count <= index)
		{
			values.Add(TextParam.Value(null));
		}
		values[index] = value;
		Refresh();
	}

	private protected override void RefreshKind(bool languageChanged)
	{
		if (languageChanged || values.Exists(value => value.IsBound))
		{
			Refresh();
		}
	}

	/// <summary>Writes the key's text with its values, and shows it: the same rows again, or new ones when its shape changed.</summary>
	private void Refresh()
	{
		var named = new (string Name, object? Value)[values.Count];
		for (var i = 0; i < values.Count; i++)
		{
			var shown = values[i].InputAction != null ? $"{ActionStart}{i}{ActionEnd}" : values[i].Now;
			named[i] = (i.ToString(System.Globalization.CultureInfo.InvariantCulture), shown);
		}
		var wanted = Split(key.Format(named));
		if (SameShape(wanted))
		{
			ShowTexts(wanted);
			return;
		}
		Rebuild(wanted);
	}

	private bool SameShape(List<LabelPart> wanted)
	{
		if (wanted.Count != parts.Count)
		{
			return false;
		}
		for (var i = 0; i < wanted.Count; i++)
		{
			if (wanted[i].Action != parts[i].Action)
			{
				return false;
			}
		}
		return true;
	}

	private void ShowTexts(List<LabelPart> wanted)
	{
		for (var i = 0; i < wanted.Count; i++)
		{
			if (wanted[i].Action < 0)
			{
				Children[i].ShowPlainText(wanted[i].Text);
			}
		}
	}

	private void Rebuild(List<LabelPart> wanted)
	{
		Clear();
		parts.Clear();
		foreach (var part in wanted)
		{
			parts.Add(part);
			if (part.Action >= 0)
			{
				Add(new PromptView(ActionName(values[part.Action].InputAction!)));
				continue;
			}

			var label = Add(new Label());
			label.AddClass("localized-label-text");
			label.ShowPlainText(part.Text);
		}
	}

	/// <summary>Stretches of text between the actions' marks, each mark an action's place among the values.</summary>
	private static List<LabelPart> Split(string written)
	{
		var found = new List<LabelPart>();
		var text = new StringBuilder();
		for (var i = 0; i < written.Length; i++)
		{
			var end = written[i] == ActionStart ? written.IndexOf(ActionEnd, i) : -1;
			if (end < 0 || !int.TryParse(written.AsSpan(i + 1, end - i - 1), out var action))
			{
				text.Append(written[i]);
				continue;
			}

			AddText(found, text);
			found.Add(new LabelPart(string.Empty, action));
			i = end;
		}
		AddText(found, text);
		return found;
	}

	private static void AddText(List<LabelPart> found, StringBuilder text)
	{
		var trimmed = text.ToString().Trim();
		text.Clear();
		if (trimmed.Length > 0)
		{
			found.Add(new LabelPart(trimmed, -1));
		}
	}

	private static string ActionName(InputAction action)
	{
		return string.IsNullOrEmpty(action.Map.Name) ? action.Name : $"{action.Map.Name}/{action.Name}";
	}
}
