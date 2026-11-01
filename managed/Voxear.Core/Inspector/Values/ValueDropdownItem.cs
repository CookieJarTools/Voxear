namespace Voxear
{
	/// <summary>A value a [ValueDropdown] offers, with what its menu calls it.</summary>
	public readonly struct ValueDropdownItem
	{
		public ValueDropdownItem(string text, object? value)
		{
			Text = text;
			Value = value;
		}

		public string Text { get; }

		public object? Value { get; }
	}
}
