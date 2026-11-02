namespace Voxear;

/// <summary>How a push given to a Rigidbody2D is measured, as Unity's ForceMode2D.</summary>
public enum ForceMode2D
{
	/// <summary>A force in newtons, spread over the next physics step.</summary>
	Force,

	/// <summary>A kick in newton-seconds, all at once.</summary>
	Impulse,
}
