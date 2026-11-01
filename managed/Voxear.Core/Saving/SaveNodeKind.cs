namespace Voxear
{
	/// <summary>What a SaveNode holds.</summary>
	public enum SaveNodeKind
	{
		/// <summary>Nothing: a null reference, or a value the save does not have.</summary>
		Null,
		Bool,
		/// <summary>A whole number, kept as a long.</summary>
		Integer,
		/// <summary>A number with a fraction, kept as a double.</summary>
		Float,
		Text,
		/// <summary>Raw bytes, such as a byte[] field. JSON writes them as base64 text.</summary>
		Bytes,
		/// <summary>Values in order, as an array or a List keeps them.</summary>
		List,
		/// <summary>Named values in order, as a class or struct keeps its fields.</summary>
		Table,
	}
}
