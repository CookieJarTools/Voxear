using System;
using System.Collections.Generic;
using System.Text.Json;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The look packs a player can wear, and the one they wear. A pack is laid over whatever lighting a level has, so
/// the choice goes with the player from level to level, and it is kept beside the saves, so the game starts in it
/// again.
/// <code>
/// foreach (var pack in LookPacks.All)
/// {
///     menu.Add(new Button(pack.Name)).Clicked += () => LookPacks.Use(pack);
/// }
/// </code>
/// <para>
/// All lists the engine's own packs, the project's <c>.lookpack</c> assets, and any the player put in the game's
/// Looks folder (Folder). LookPackScreen is a ready-made menu of them.
/// </para>
/// </summary>
public static class LookPacks
{
	/// <summary>The pack worn changed, by Use or UseNone: the one worn now, or LookPack.None.</summary>
	public static event Action<LookPack>? Changed;

	/// <summary>
	/// Every pack there is to wear: the engine's own, then the game's, then the player's, each by name. Read afresh
	/// each time, so a file the player just added is there.
	/// </summary>
	public static IReadOnlyList<LookPack> All => Read().Packs;

	/// <summary>The pack worn; LookPack.None for none.</summary>
	public static LookPack Current => Read().Worn;

	/// <summary>
	/// The folder a player puts <c>.lookpack</c> files of their own in, beside the game's saves; null where there is
	/// nowhere to keep them.
	/// </summary>
	public static string? Folder => Read().Folder;

	/// <summary>
	/// Wears a pack from the next frame, over every level until another is worn. The choice is kept for the game's
	/// next run unless <paramref name="remember"/> is false, as for a preview. A pack that cannot be read is
	/// reported in the Console, and the one worn stays.
	/// </summary>
	public static void Use(LookPack pack, bool remember = true)
	{
		if (!pack.IsSet)
		{
			UseNone(remember);
			return;
		}

		NativeLooks.Wear(pack.Reference, remember);
		Changed?.Invoke(pack);
	}

	/// <summary>Takes off the pack worn: every level is drawn with its own lighting alone.</summary>
	public static void UseNone(bool remember = true)
	{
		NativeLooks.Wear(string.Empty, remember);
		Changed?.Invoke(LookPack.None);
	}

	private static (List<LookPack> Packs, LookPack Worn, string? Folder) Read()
	{
		var packs = new List<LookPack>();
		var json = NativeLooks.Info();
		if (json == null)
		{
			return (packs, LookPack.None, null);
		}

		using var document = JsonDocument.Parse(json);
		var root = document.RootElement;
		foreach (var entry in root.GetProperty("packs").EnumerateArray())
		{
			var source = entry.GetProperty("source").GetString() switch
			{
				"game" => LookPackSource.Game,
				"player" => LookPackSource.Player,
				_ => LookPackSource.BuiltIn,
			};
			packs.Add(new LookPack(Text(entry, "reference"), Text(entry, "name"), Text(entry, "description"), source));
		}

		var named = root.GetProperty("worn").GetString();
		var worn = LookPack.None;
		if (!string.IsNullOrEmpty(named))
		{
			var listed = packs.FindIndex(pack => pack.Reference == named);
			worn = listed >= 0 ? packs[listed] : LookPack.Load(named);
		}

		return (packs, worn, root.GetProperty("folder").GetString());
	}

	private static string Text(JsonElement entry, string name)
	{
		return entry.GetProperty(name).GetString() ?? string.Empty;
	}
}
