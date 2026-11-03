using System;
using System.Globalization;

namespace Voxear;

/// <summary>A value dragged between the least and the greatest, in steps when given one.</summary>
public class Slider : UiElement
{
	private readonly float min;
	private readonly float max;
	private readonly float step;
	private Action<float>? moved;

	public Slider(float min = 0f, float max = 1f, float value = 0f, float step = 0f) : base(UiKind.Slider, string.Empty)
	{
		(this.min, this.max, this.step) = (min, max, step);
		Value = Math.Clamp(value, min, Math.Max(min, max));
	}

	/// <summary>Runs with the value each time it is moved on screen.</summary>
	public event Action<float>? Moved
	{
		add
		{
			moved += value;
			Listen();
		}
		remove
		{
			moved -= value;
		}
	}

	/// <summary>Runs `handler` with the value each time it is moved on screen, and gives the slider back.</summary>
	public Slider OnMoved(Action<float> handler)
	{
		Moved += handler;
		return this;
	}

	/// <summary>Puts it where `value` says, now and each time it is drawn again: <c>bar.Bind(() => Value.Current)</c>.</summary>
	public Slider Bind(Func<float> value)
	{
		BindValue(value);
		return this;
	}

	private protected override string MadeFrom => string.Create(CultureInfo.InvariantCulture, $"{min} {max} {step}");

	private protected override bool KindListens => moved != null;

	private protected override void RaiseKindChanged()
	{
		moved?.Invoke(Value);
	}
}
