using System;

namespace Voxear
{
	/// <summary>
	/// Saves a type in a way of its own, in place of its fields: a type from a library whose fields are private, say,
	/// or one better kept as text. Derive from SaveConverter&lt;T&gt;, and the game finds the class by itself; one
	/// added with Save.AddConverter wins over it.
	/// </summary>
	public abstract class SaveConverter
	{
		private protected SaveConverter()
		{
		}

		internal abstract Type Type { get; }

		internal abstract SaveNode WriteObject(object value);

		internal abstract object? ReadObject(SaveNode node);
	}
}
