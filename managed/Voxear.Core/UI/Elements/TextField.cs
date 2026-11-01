using System;

namespace Voxear
{
	/// <summary>A line to type into. Its Text is what was typed.</summary>
	public class TextField : UiElement
	{
		public TextField(string text = "") : base(UiKind.TextField, text)
		{
		}

		/// <summary>Runs with the text each time it is typed in.</summary>
		public event Action<string>? Typed;

		private protected override void OnChanged()
		{
			Typed?.Invoke(Text);
		}
	}
}
