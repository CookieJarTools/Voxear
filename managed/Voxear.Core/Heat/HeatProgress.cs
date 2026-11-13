namespace Voxear;

/// <summary>How far one of a Heat Body's changes has counted, as HeatBody.Progress gives it.</summary>
public readonly struct HeatProgress
{
	internal HeatProgress(float held, float needed, float share, bool done)
	{
		Held = held;
		Needed = needed;
		Share = share;
		Done = done;
	}

	/// <summary>How many seconds it has been hot or cold enough.</summary>
	public float Held { get; }

	/// <summary>How many seconds it takes.</summary>
	public float Needed { get; }

	/// <summary>How far along it is, from 0 to 1: a cooking bar's fill.</summary>
	public float Share { get; }

	/// <summary>Whether it has happened.</summary>
	public bool Done { get; }
}
