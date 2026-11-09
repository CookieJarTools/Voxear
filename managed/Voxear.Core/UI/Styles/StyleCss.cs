using System;
using System.Globalization;

namespace Voxear;

/// <summary>What each of UiStyle's values is as a style sheet writes it, which the engine reads.</summary>
internal static class StyleCss
{
	public static string Of(Position value)
	{
		return value switch
		{
			Position.Relative => "relative",
			Position.Absolute => "absolute",
			_ => throw new ArgumentOutOfRangeException(nameof(value)),
		};
	}

	public static string Of(FlexDirection value)
	{
		return value switch
		{
			FlexDirection.Column => "column",
			FlexDirection.Row => "row",
			FlexDirection.ColumnReverse => "column-reverse",
			FlexDirection.RowReverse => "row-reverse",
			_ => throw new ArgumentOutOfRangeException(nameof(value)),
		};
	}

	public static string Of(FlexWrap value)
	{
		return value switch
		{
			FlexWrap.NoWrap => "nowrap",
			FlexWrap.Wrap => "wrap",
			_ => throw new ArgumentOutOfRangeException(nameof(value)),
		};
	}

	public static string Of(Align value)
	{
		return value switch
		{
			Align.Start => "flex-start",
			Align.Center => "center",
			Align.End => "flex-end",
			Align.Stretch => "stretch",
			Align.Baseline => "baseline",
			_ => throw new ArgumentOutOfRangeException(nameof(value)),
		};
	}

	public static string Of(Justify value)
	{
		return value switch
		{
			Justify.Start => "flex-start",
			Justify.Center => "center",
			Justify.End => "flex-end",
			Justify.SpaceBetween => "space-between",
			Justify.SpaceAround => "space-around",
			Justify.SpaceEvenly => "space-evenly",
			_ => throw new ArgumentOutOfRangeException(nameof(value)),
		};
	}

	public static string Of(DisplayStyle value)
	{
		return value switch
		{
			DisplayStyle.Flex => "flex",
			DisplayStyle.Grid => "grid",
			DisplayStyle.None => "none",
			_ => throw new ArgumentOutOfRangeException(nameof(value)),
		};
	}

	public static string Of(Overflow value)
	{
		return value switch
		{
			Overflow.Visible => "visible",
			Overflow.Hidden => "hidden",
			Overflow.Scroll => "scroll",
			_ => throw new ArgumentOutOfRangeException(nameof(value)),
		};
	}

	public static string Of(FontWeight value)
	{
		return value switch
		{
			FontWeight.Normal => "normal",
			FontWeight.Medium => "medium",
			FontWeight.SemiBold => "semibold",
			FontWeight.Bold => "bold",
			_ => throw new ArgumentOutOfRangeException(nameof(value)),
		};
	}

	public static string Of(FontFamily value)
	{
		return value switch
		{
			FontFamily.Sans => "sans",
			FontFamily.Mono => "mono",
			_ => throw new ArgumentOutOfRangeException(nameof(value)),
		};
	}

	public static string Of(TextAlign value)
	{
		return value switch
		{
			TextAlign.Left => "left",
			TextAlign.Center => "center",
			TextAlign.Right => "right",
			_ => throw new ArgumentOutOfRangeException(nameof(value)),
		};
	}

	public static string Of(PointerEvents value)
	{
		return value switch
		{
			PointerEvents.Auto => "auto",
			PointerEvents.None => "none",
			_ => throw new ArgumentOutOfRangeException(nameof(value)),
		};
	}

	public static string Of(Length value)
	{
		return value.ToString();
	}

	public static string Of(Color value)
	{
		return $"#{Channel(value.R):x2}{Channel(value.G):x2}{Channel(value.B):x2}{Channel(value.A):x2}";
	}

	public static string Pixels(float value)
	{
		return $"{Number(value)}px";
	}

	public static string Number(float value)
	{
		return value.ToString(CultureInfo.InvariantCulture);
	}

	public static string Whole(int value)
	{
		return value.ToString(CultureInfo.InvariantCulture);
	}

	private static int Channel(float value)
	{
		return (int)MathF.Round(Math.Clamp(value, 0f, 1f) * 255f);
	}
}
