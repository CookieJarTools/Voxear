namespace Voxear;

/// <summary>How carefully a Nav Agent goes round other agents and characters.</summary>
public enum NavAvoidance
{
	/// <summary>It does not, though others go round it.</summary>
	None,

	/// <summary>Heeding the four nearest.</summary>
	Low,

	/// <summary>Heeding the ten nearest.</summary>
	High,
}
