namespace Voxear;

/// <summary>Why a save could not be read or written. The first five mirror the engine's numbers for them.</summary>
public enum SaveError
{
	/// <summary>There is no save by that name.</summary>
	Missing = 1,

	/// <summary>The file is not a save, was cut short, or its bytes changed after it was written.</summary>
	Damaged = 2,

	/// <summary>The disk said no: it is full, or the save folder cannot be written.</summary>
	Failed = 3,

	/// <summary>The name cannot be a file's: use letters, digits, spaces, - and _.</summary>
	BadName = 4,

	/// <summary>There is nowhere to keep saves here.</summary>
	NoFolder = 5,

	/// <summary>A newer version of the game wrote it, in a shape this one cannot read.</summary>
	NewerVersion = 6,

	/// <summary>A value in it cannot be saved or read, such as a delegate, or a type no longer in the game.</summary>
	Unsupported = 7,
}
