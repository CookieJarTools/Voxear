using System;

namespace Voxear;

/// <summary>Sets several of an element's style properties at once, keeping the rest.</summary>
public static class UiStyleExtensions
{
	extension<T>(T element) where T : UiElement
	{
		/// <summary>
		/// Sets the style properties given, by name, keeping the rest, and gives the element back, so it chains inside
		/// Add: <c>bar.SetStyles(position: Position.Absolute, left: 20, top: 20, flexDirection: FlexDirection.Row, gap: 12)</c>.
		/// Each is as Style has it.
		/// </summary>
		public T SetStyles(
			DisplayStyle? display = null,
			Position? position = null,
			Length? left = null,
			Length? top = null,
			Length? right = null,
			Length? bottom = null,
			Length? width = null,
			Length? height = null,
			Length? minWidth = null,
			Length? minHeight = null,
			Length? maxWidth = null,
			Length? maxHeight = null,
			Length? margin = null,
			Length? marginLeft = null,
			Length? marginTop = null,
			Length? marginRight = null,
			Length? marginBottom = null,
			Length? padding = null,
			Length? paddingLeft = null,
			Length? paddingTop = null,
			Length? paddingRight = null,
			Length? paddingBottom = null,
			Length? gap = null,
			Length? rowGap = null,
			Length? columnGap = null,
			FlexDirection? flexDirection = null,
			float? flexGrow = null,
			float? flexShrink = null,
			Length? flexBasis = null,
			FlexWrap? flexWrap = null,
			Align? alignItems = null,
			Align? alignSelf = null,
			Justify? justifyContent = null,
			Overflow? overflow = null,
			Color? backgroundColor = null,
			float? borderWidth = null,
			Color? borderColor = null,
			float? borderRadius = null,
			float? opacity = null,
			int? zIndex = null,
			PointerEvents? pointerEvents = null,
			Color? color = null,
			float? fontSize = null,
			FontWeight? fontWeight = null,
			FontFamily? fontFamily = null,
			TextAlign? textAlign = null,
			float? lineHeight = null)
		{
			var style = element.Style;
			Put(display, value => style.Display = value);
			Put(position, value => style.Position = value);
			Put(left, value => style.Left = value);
			Put(top, value => style.Top = value);
			Put(right, value => style.Right = value);
			Put(bottom, value => style.Bottom = value);
			Put(width, value => style.Width = value);
			Put(height, value => style.Height = value);
			Put(minWidth, value => style.MinWidth = value);
			Put(minHeight, value => style.MinHeight = value);
			Put(maxWidth, value => style.MaxWidth = value);
			Put(maxHeight, value => style.MaxHeight = value);
			Put(margin, value => style.Margin = value);
			Put(marginLeft, value => style.MarginLeft = value);
			Put(marginTop, value => style.MarginTop = value);
			Put(marginRight, value => style.MarginRight = value);
			Put(marginBottom, value => style.MarginBottom = value);
			Put(padding, value => style.Padding = value);
			Put(paddingLeft, value => style.PaddingLeft = value);
			Put(paddingTop, value => style.PaddingTop = value);
			Put(paddingRight, value => style.PaddingRight = value);
			Put(paddingBottom, value => style.PaddingBottom = value);
			Put(gap, value => style.Gap = value);
			Put(rowGap, value => style.RowGap = value);
			Put(columnGap, value => style.ColumnGap = value);
			Put(flexDirection, value => style.FlexDirection = value);
			Put(flexGrow, value => style.FlexGrow = value);
			Put(flexShrink, value => style.FlexShrink = value);
			Put(flexBasis, value => style.FlexBasis = value);
			Put(flexWrap, value => style.FlexWrap = value);
			Put(alignItems, value => style.AlignItems = value);
			Put(alignSelf, value => style.AlignSelf = value);
			Put(justifyContent, value => style.JustifyContent = value);
			Put(overflow, value => style.Overflow = value);
			Put(backgroundColor, value => style.BackgroundColor = value);
			Put(borderWidth, value => style.BorderWidth = value);
			Put(borderColor, value => style.BorderColor = value);
			Put(borderRadius, value => style.BorderRadius = value);
			Put(opacity, value => style.Opacity = value);
			Put(zIndex, value => style.ZIndex = value);
			Put(pointerEvents, value => style.PointerEvents = value);
			Put(color, value => style.Color = value);
			Put(fontSize, value => style.FontSize = value);
			Put(fontWeight, value => style.FontWeight = value);
			Put(fontFamily, value => style.FontFamily = value);
			Put(textAlign, value => style.TextAlign = value);
			Put(lineHeight, value => style.LineHeight = value);
			return element;
		}
	}

	private static void Put<TValue>(TValue? given, Action<TValue> set) where TValue : struct
	{
		if (given is { } value)
		{
			set(value);
		}
	}
}
