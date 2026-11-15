using Voxear;

namespace {{Namespace}};

/// <summary>
/// Gives the Map on its Obj an Open Map action when the game starts, where the Map has none picked: Open, or else
/// Map/Open Map. Picking the action on the Map itself does the same.
/// </summary>
public sealed class MapOpenAction : Behaviour
{
	[SerializeField, Tooltip("The action that opens and closes the map; none for Map/Open Map")] private InputActionRef open;

	protected override void Start()
	{
		var map = Obj.Map;
		if (map.IsValid && !map.OpenAction.IsSet)
		{
			map.OpenAction = open.IsSet ? open : new InputActionRef("Map/Open Map");
		}
	}
}
