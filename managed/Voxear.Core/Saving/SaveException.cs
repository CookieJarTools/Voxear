using System;

namespace Voxear;

/// <summary>A save that could not be read or written. Error says why, in a form code can check.</summary>
public sealed class SaveException : Exception
{
	public SaveException(SaveError error, string message) : base(message)
	{
		Error = error;
	}

	public SaveException(SaveError error, string message, Exception inner) : base(message, inner)
	{
		Error = error;
	}

	public SaveError Error { get; }
}
