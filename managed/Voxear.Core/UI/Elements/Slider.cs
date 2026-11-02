using System;
using System.Globalization;

namespace Voxear;

/// <summary>A value dragged between the least and the greatest, in steps when given one.</summary>
public class Slider : UiElement
{
	private readonly float min;
	private readonly float max;
	private readonly float step;

	public Slider(float min = 0f, float max = 1f, float value = 0f, float step = 0f) : base(UiKind.Slider, string.Empty)
	{
		(this.min, this.max, this.step) = (min, max, step);
		Value = Math.Clamp(value, min, Math.Max(min, max));
	}

	/// <summary>Runs with the value each time it is moved on screen.</summary>
	public event Action<float>? Moved;

	private protected override string MadeFrom => string.Create(CultureInfo.InvariantCulture, $"{min} {max} {step}");

	private protected override void OnChanged()
	{
		Moved?.Invoke(Value);
	}
}
