using System;
using System.Collections.Generic;

namespace Voxear;

/// <summary>
/// One of the worn shader pack's options, as its menu shows it: a number on a slider, a toggle or a choice, with its
/// value now. Set it with ShaderPacks.SetOption.
/// </summary>
public sealed class ShaderPackOption
{
	internal ShaderPackOption(
		string name,
		string label,
		string about,
		ShaderPackOptionKind kind,
		(float Low, float High) range,
		IReadOnlyList<string> choices,
		float value)
	{
		Name = name;
		Label = label;
		About = about;
		Kind = kind;
		Low = range.Low;
		High = range.High;
		Choices = choices;
		Value = value;
	}

	/// <summary>What names it, as the pack's programs and ShaderPacks.SetOption do.</summary>
	public string Name { get; }

	/// <summary>What a menu calls it.</summary>
	public string Label { get; }

	/// <summary>A line saying what it does, for a tooltip; empty for none.</summary>
	public string About { get; }

	public ShaderPackOptionKind Kind { get; }

	/// <summary>A number's least and greatest; 0 and 1 for a toggle, and the first and last place for a choice.</summary>
	public float Low { get; }

	public float High { get; }

	/// <summary>A choice's choices, in order; empty for a number or a toggle.</summary>
	public IReadOnlyList<string> Choices { get; }

	/// <summary>Its value now: a number as it is, a toggle 0 or 1, a choice its place among the choices.</summary>
	public float Value { get; }

	/// <summary>A toggle's value: whether it is on.</summary>
	public bool IsOn => Value >= 0.5f;

	/// <summary>A choice's value: the choice picked; empty for a number or a toggle.</summary>
	public string Chosen => Kind == ShaderPackOptionKind.Choice && Choices.Count > 0
		? Choices[Math.Clamp((int)MathF.Round(Value), 0, Choices.Count - 1)]
		: string.Empty;

	public override string ToString()
	{
		return Kind switch
		{
			ShaderPackOptionKind.Toggle => $"{Label}: {(IsOn ? "on" : "off")}",
			ShaderPackOptionKind.Choice => $"{Label}: {Chosen}",
			_ => $"{Label}: {Value:0.##}",
		};
	}
}
