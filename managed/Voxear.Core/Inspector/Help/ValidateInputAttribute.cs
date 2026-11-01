using System;

namespace Voxear
{
	/// <summary>
	/// Asks a method of the behaviour, named with nameof, whether the field's value is right: it takes the value and
	/// returns bool, or string, empty when it is right and otherwise what is wrong with it. While it is not, the
	/// Inspector shows an error on the field, saying what the method said or else message.
	/// </summary>
	[AttributeUsage(AttributeTargets.Field)]
	public sealed class ValidateInputAttribute : Attribute
	{
		public ValidateInputAttribute(string method, string? message = null)
		{
			Method = method;
			Message = message;
		}

		public string Method { get; }

		/// <summary>What the error says when the method answers false, instead of "{Field} is not valid".</summary>
		public string? Message { get; }
	}
}
