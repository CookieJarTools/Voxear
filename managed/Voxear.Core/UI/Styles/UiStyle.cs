using System;
using System.Collections.Generic;

namespace Voxear;

/// <summary>
/// An element's own style, set a property at a time, as a style attribute would be: each wins over the sheets, and
/// setting one keeps the rest, the style attribute's too. <c>bar.Style.Position = Position.Absolute;</c>,
/// <c>bar.Style.Left = 20;</c>, <c>bar.Style.Width = Length.Percent(50);</c>, and to hand a property back to the
/// sheets, <c>bar.Style.Width = null;</c>. Reading one gives what code set, or null: what the sheets give is not
/// read back. SetStyles sets several at once, and SetStyle takes CSS text for what has no property here yet.
/// </summary>
public sealed class UiStyle
{
	private readonly UiElement element;
	private readonly Dictionary<string, (object Value, string Css)> values = [];
	private readonly List<string> order = [];

	internal UiStyle(UiElement element)
	{
		this.element = element;
	}

	/// <summary>How it lays out what is in it, or None to hide it, taking no space.</summary>
	public DisplayStyle? Display
	{
		get { return Get<DisplayStyle>("display"); }
		set { Set("display", value, StyleCss.Of); }
	}

	/// <summary>In line with the others, or Absolute, placed by Left, Top, Right and Bottom.</summary>
	public Position? Position
	{
		get { return Get<Position>("position"); }
		set { Set("position", value, StyleCss.Of); }
	}

	/// <summary>Where its left edge goes: within the box it is in when Absolute, or moved by that much.</summary>
	public Length? Left
	{
		get { return Get<Length>("left"); }
		set { Set("left", value, StyleCss.Of); }
	}

	/// <summary>Where its top edge goes: within the box it is in when Absolute, or moved by that much.</summary>
	public Length? Top
	{
		get { return Get<Length>("top"); }
		set { Set("top", value, StyleCss.Of); }
	}

	/// <summary>Where its right edge goes, from the right of the box it is in, when Absolute.</summary>
	public Length? Right
	{
		get { return Get<Length>("right"); }
		set { Set("right", value, StyleCss.Of); }
	}

	/// <summary>Where its bottom edge goes, from the bottom of the box it is in, when Absolute.</summary>
	public Length? Bottom
	{
		get { return Get<Length>("bottom"); }
		set { Set("bottom", value, StyleCss.Of); }
	}

	/// <summary>How wide it is.</summary>
	public Length? Width
	{
		get { return Get<Length>("width"); }
		set { Set("width", value, StyleCss.Of); }
	}

	/// <summary>How high it is.</summary>
	public Length? Height
	{
		get { return Get<Length>("height"); }
		set { Set("height", value, StyleCss.Of); }
	}

	/// <summary>The narrowest it is laid out.</summary>
	public Length? MinWidth
	{
		get { return Get<Length>("min-width"); }
		set { Set("min-width", value, StyleCss.Of); }
	}

	/// <summary>The lowest it is laid out.</summary>
	public Length? MinHeight
	{
		get { return Get<Length>("min-height"); }
		set { Set("min-height", value, StyleCss.Of); }
	}

	/// <summary>The widest it is laid out.</summary>
	public Length? MaxWidth
	{
		get { return Get<Length>("max-width"); }
		set { Set("max-width", value, StyleCss.Of); }
	}

	/// <summary>The highest it is laid out.</summary>
	public Length? MaxHeight
	{
		get { return Get<Length>("max-height"); }
		set { Set("max-height", value, StyleCss.Of); }
	}

	/// <summary>Space outside its edges, on every side.</summary>
	public Length? Margin
	{
		get { return Get<Length>("margin"); }
		set { Set("margin", value, StyleCss.Of); }
	}

	/// <summary>Space outside its left edge.</summary>
	public Length? MarginLeft
	{
		get { return Get<Length>("margin-left"); }
		set { Set("margin-left", value, StyleCss.Of); }
	}

	/// <summary>Space outside its top edge.</summary>
	public Length? MarginTop
	{
		get { return Get<Length>("margin-top"); }
		set { Set("margin-top", value, StyleCss.Of); }
	}

	/// <summary>Space outside its right edge.</summary>
	public Length? MarginRight
	{
		get { return Get<Length>("margin-right"); }
		set { Set("margin-right", value, StyleCss.Of); }
	}

	/// <summary>Space outside its bottom edge.</summary>
	public Length? MarginBottom
	{
		get { return Get<Length>("margin-bottom"); }
		set { Set("margin-bottom", value, StyleCss.Of); }
	}

	/// <summary>Space inside its edges, on every side.</summary>
	public Length? Padding
	{
		get { return Get<Length>("padding"); }
		set { Set("padding", value, StyleCss.Of); }
	}

	/// <summary>Space inside its left edge.</summary>
	public Length? PaddingLeft
	{
		get { return Get<Length>("padding-left"); }
		set { Set("padding-left", value, StyleCss.Of); }
	}

	/// <summary>Space inside its top edge.</summary>
	public Length? PaddingTop
	{
		get { return Get<Length>("padding-top"); }
		set { Set("padding-top", value, StyleCss.Of); }
	}

	/// <summary>Space inside its right edge.</summary>
	public Length? PaddingRight
	{
		get { return Get<Length>("padding-right"); }
		set { Set("padding-right", value, StyleCss.Of); }
	}

	/// <summary>Space inside its bottom edge.</summary>
	public Length? PaddingBottom
	{
		get { return Get<Length>("padding-bottom"); }
		set { Set("padding-bottom", value, StyleCss.Of); }
	}

	/// <summary>Space between the elements in it.</summary>
	public Length? Gap
	{
		get { return Get<Length>("gap"); }
		set { Set("gap", value, StyleCss.Of); }
	}

	/// <summary>Space between its rows.</summary>
	public Length? RowGap
	{
		get { return Get<Length>("row-gap"); }
		set { Set("row-gap", value, StyleCss.Of); }
	}

	/// <summary>Space between its columns.</summary>
	public Length? ColumnGap
	{
		get { return Get<Length>("column-gap"); }
		set { Set("column-gap", value, StyleCss.Of); }
	}

	/// <summary>Which way it lays out what is in it: a column, the default, or a row.</summary>
	public FlexDirection? FlexDirection
	{
		get { return Get<FlexDirection>("flex-direction"); }
		set { Set("flex-direction", value, StyleCss.Of); }
	}

	/// <summary>How much of the space left over it takes, against the others: 1 takes it all, alone.</summary>
	public float? FlexGrow
	{
		get { return Get<float>("flex-grow"); }
		set { Set("flex-grow", value, StyleCss.Number); }
	}

	/// <summary>How much it gives up when there is not room, against the others.</summary>
	public float? FlexShrink
	{
		get { return Get<float>("flex-shrink"); }
		set { Set("flex-shrink", value, StyleCss.Number); }
	}

	/// <summary>Its size along the line before space is shared out.</summary>
	public Length? FlexBasis
	{
		get { return Get<Length>("flex-basis"); }
		set { Set("flex-basis", value, StyleCss.Of); }
	}

	/// <summary>Whether what it holds goes onto another line when it does not fit.</summary>
	public FlexWrap? FlexWrap
	{
		get { return Get<FlexWrap>("flex-wrap"); }
		set { Set("flex-wrap", value, StyleCss.Of); }
	}

	/// <summary>Where what it holds sits across its line.</summary>
	public Align? AlignItems
	{
		get { return Get<Align>("align-items"); }
		set { Set("align-items", value, StyleCss.Of); }
	}

	/// <summary>Where it sits across its line, in place of what the box it is in says.</summary>
	public Align? AlignSelf
	{
		get { return Get<Align>("align-self"); }
		set { Set("align-self", value, StyleCss.Of); }
	}

	/// <summary>Where what it holds sits along its line, and how the space left over is shared.</summary>
	public Justify? JustifyContent
	{
		get { return Get<Justify>("justify-content"); }
		set { Set("justify-content", value, StyleCss.Of); }
	}

	/// <summary>What shows of what it holds past its edges.</summary>
	public Overflow? Overflow
	{
		get { return Get<Overflow>("overflow"); }
		set { Set("overflow", value, StyleCss.Of); }
	}

	/// <summary>Its fill.</summary>
	public Color? BackgroundColor
	{
		get { return Get<Color>("background-color"); }
		set { Set("background-color", value, StyleCss.Of); }
	}

	/// <summary>How wide its border is, in pixels.</summary>
	public float? BorderWidth
	{
		get { return Get<float>("border-width"); }
		set { Set("border-width", value, StyleCss.Pixels); }
	}

	/// <summary>Its border's colour.</summary>
	public Color? BorderColor
	{
		get { return Get<Color>("border-color"); }
		set { Set("border-color", value, StyleCss.Of); }
	}

	/// <summary>How round its corners are, in pixels.</summary>
	public float? BorderRadius
	{
		get { return Get<float>("border-radius"); }
		set { Set("border-radius", value, StyleCss.Pixels); }
	}

	/// <summary>How much of it shows, from 0, none, to 1.</summary>
	public float? Opacity
	{
		get { return Get<float>("opacity"); }
		set { Set("opacity", value, StyleCss.Number); }
	}

	/// <summary>Which of its siblings draws over which: higher draws over lower.</summary>
	public int? ZIndex
	{
		get { return Get<int>("z-index"); }
		set { Set("z-index", value, StyleCss.Whole); }
	}

	/// <summary>Whether it takes the pointer, or clicks pass through it.</summary>
	public PointerEvents? PointerEvents
	{
		get { return Get<PointerEvents>("pointer-events"); }
		set { Set("pointer-events", value, StyleCss.Of); }
	}

	/// <summary>Its text's colour.</summary>
	public Color? Color
	{
		get { return Get<Color>("color"); }
		set { Set("color", value, StyleCss.Of); }
	}

	/// <summary>How large its text is, in pixels.</summary>
	public float? FontSize
	{
		get { return Get<float>("font-size"); }
		set { Set("font-size", value, StyleCss.Pixels); }
	}

	/// <summary>How heavy its text is.</summary>
	public FontWeight? FontWeight
	{
		get { return Get<FontWeight>("font-weight"); }
		set { Set("font-weight", value, StyleCss.Of); }
	}

	/// <summary>Which font its text uses.</summary>
	public FontFamily? FontFamily
	{
		get { return Get<FontFamily>("font-family"); }
		set { Set("font-family", value, StyleCss.Of); }
	}

	/// <summary>Where its text sits across it.</summary>
	public TextAlign? TextAlign
	{
		get { return Get<TextAlign>("text-align"); }
		set { Set("text-align", value, StyleCss.Of); }
	}

	/// <summary>How far apart its lines of text are, in pixels.</summary>
	public float? LineHeight
	{
		get { return Get<float>("line-height"); }
		set { Set("line-height", value, StyleCss.Pixels); }
	}

	/// <summary>Sends every property set, in the order they were set, as the element comes on screen.</summary>
	internal void SendAll()
	{
		foreach (var name in order)
		{
			element.SendStyleProperty(name, values[name].Css);
		}
	}

	/// <summary>Lets go of what was set, as SetStyle replaces the whole style.</summary>
	internal void Forget()
	{
		values.Clear();
		order.Clear();
	}

	private T? Get<T>(string name) where T : struct
	{
		return values.TryGetValue(name, out var set) ? (T)set.Value : null;
	}

	/// <summary>Sets one property, after the others, so it wins over one set before that overlaps it, or takes it out.</summary>
	private void Set<T>(string name, T? value, Func<T, string> css) where T : struct
	{
		order.Remove(name);
		if (value is not { } given)
		{
			values.Remove(name);
			element.SendStyleProperty(name, string.Empty);
			return;
		}

		var text = css(given);
		values[name] = (given, text);
		order.Add(name);
		element.SendStyleProperty(name, text);
	}
}
