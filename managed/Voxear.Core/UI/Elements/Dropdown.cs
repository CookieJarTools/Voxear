using System;
using System.Collections.Generic;

namespace Voxear;

/// <summary>A choice of options, one picked at a time; Selected is the one picked, by its place.</summary>
public class Dropdown : UiElement
{
	private readonly string[] options;

	public Dropdown(params string[] options) : base(UiKind.Dropdown, string.Empty)
	{
		this.options = options;
	}

	/// <summary>Runs with the option picked, by its place, each time one is picked on screen.</summary>
	public event Action<int>? Picked;

	public IReadOnlyList<string> Options => options;

	/// <summary>The option picked, by its place in Options.</summary>
	public int Selected
	{
		get { return (int)Value; }
		set { Value = value; }
	}

	private protected override string MadeFrom => string.Join('\n', options);

	private protected override void OnChanged()
	{
		Picked?.Invoke(Selected);
	}
}
