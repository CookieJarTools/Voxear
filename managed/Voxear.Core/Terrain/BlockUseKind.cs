namespace Voxear;

/// <summary>
/// What using a block did: a door, a trap door or a gate opened or shut; a lever turned on or off; a button was
/// pressed; or a block marked usable said it was used, for a script to answer.
/// </summary>
public enum BlockUseKind
{
	None,
	Opens,
	Toggles,
	Presses,
	Says,
}
