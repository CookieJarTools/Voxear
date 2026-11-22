namespace Voxear;

/// <summary>
/// A kind of route of the game's own, past road, sea and air: a caravan trail, a pilgrims' way, a smugglers' path. It
/// is nothing but its name, the file's. A Route of kind Custom names one, and Routes.Find and Routes.Nearest ask for
/// routes of it. A game adds a kind by making a data asset, with no code.
/// </summary>
[CreateAsset("World/Route Kind")]
public class CustomRouteKind : Definition
{
}
