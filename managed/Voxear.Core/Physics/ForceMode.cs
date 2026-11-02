namespace Voxear;

/// <summary>How a push given to a Rigidbody is measured, as Unity's ForceMode.</summary>
public enum ForceMode
{
	/// <summary>A force in newtons, spread over the next physics step. A heavier body moves less.</summary>
	Force,

	/// <summary>A change of speed in metres per second each second, spread over the next step, whatever the mass.</summary>
	Acceleration,

	/// <summary>A kick in newton-seconds, all at once. A heavier body moves less.</summary>
	Impulse,

	/// <summary>A change of speed in metres per second, all at once, whatever the mass.</summary>
	VelocityChange,
}
