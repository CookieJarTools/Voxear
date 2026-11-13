using System;
using System.Collections.Generic;
using System.Text.Json;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The texture packs a player can wear, and the one they wear. A pack draws the blocks and materials it names with
/// pictures of its own in every level, so the choice goes with the player from level to level, and it is kept
/// beside the saves, so the game starts in it again.
/// <code>
/// foreach (var pack in TexturePacks.All)
/// {
///     menu.Add(new Button(pack.Name)).Clicked += () => TexturePacks.Use(pack);
/// }
/// </code>
/// <para>
/// All lists the engine's own packs, the project's <c>.texturepack</c> assets, and any the player put in the game's
/// Textures folder (Folder). TexturePackScreen is a ready-made menu of them.
/// </para>
/// </summary>
public static class TexturePacks
{
	/// <summary>The pack worn changed, by Use or UseNone: the one worn now, or TexturePack.None.</summary>
	public static event Action<TexturePack>? Changed;

	/// <summary>
	/// Every pack there is to wear: the engine's own, then the game's, then the player's, each by name. Read afresh
	/// each time, so a folder the player just added is there.
	/// </summary>
	public static IReadOnlyList<TexturePack> All => Read().Packs;

	/// <summary>The pack worn; TexturePack.None for none.</summary>
	public static TexturePack Current => Read().Worn;

	/// <summary>
	/// The folder a player puts packs of their own in, beside the game's saves, each a folder holding a
	/// <c>.texturepack</c> and its pictures; null where there is nowhere to keep them.
	/// </summary>
	public static string? Folder => Read().Folder;

	/// <summary>
	/// Wears a pack from the next frame, in every level until another is worn. The choice is kept for the game's
	/// next run unless <paramref name="remember"/> is false, as for a preview. A pack that cannot be read is
	/// reported in the Console, and the one worn stays.
	/// </summary>
	public static void Use(TexturePack pack, bool remember = true)
	{
		if (!pack.IsSet)
		{
			UseNone(remember);
			return;
		}

		NativeTextures.Wear(pack.Reference, remember);
		Changed?.Invoke(pack);
	}

	/// <summary>Takes off the pack worn: every block and material is drawn with its own pictures.</summary>
	public static void UseNone(bool remember = true)
	{
		NativeTextures.Wear(string.Empty, remember);
		Changed?.Invoke(TexturePack.None);
	}

	/// <summary>
	/// Advanced: makes a pack for the player to start from, in a folder called <paramref name="name"/> in Folder. It
	/// lists every block of the game, each block of a plain colour with a small picture of its colour to paint
	/// over. Gives the pack, ready to wear, or TexturePack.None where it could not be made, as when one of that
	/// name is there already.
	/// </summary>
	public static TexturePack WriteStarter(string name)
	{
		var reference = NativeTextures.Starter(name);
		return string.IsNullOrEmpty(reference)
			? TexturePack.None
			: new TexturePack(reference, name, string.Empty, TexturePackSource.Player);
	}

	private static (List<TexturePack> Packs, TexturePack Worn, string? Folder) Read()
	{
		var packs = new List<TexturePack>();
		var json = NativeTextures.Info();
		if (json == null)
		{
			return (packs, TexturePack.None, null);
		}

		using var document = JsonDocument.Parse(json);
		var root = document.RootElement;
		foreach (var entry in root.GetProperty("packs").EnumerateArray())
		{
			var source = entry.GetProperty("source").GetString() switch
			{
				"game" => TexturePackSource.Game,
				"player" => TexturePackSource.Player,
				_ => TexturePackSource.BuiltIn,
			};
			var pack = new TexturePack(Text(entry, "reference"), Text(entry, "name"), Text(entry, "description"), source);
			packs.Add(pack);
		}

		var named = root.GetProperty("worn").GetString();
		var worn = TexturePack.None;
		if (!string.IsNullOrEmpty(named))
		{
			var listed = packs.FindIndex(pack => pack.Reference == named);
			worn = listed >= 0 ? packs[listed] : TexturePack.Load(named);
		}

		return (packs, worn, root.GetProperty("folder").GetString());
	}

	private static string Text(JsonElement entry, string name)
	{
		return entry.GetProperty(name).GetString() ?? string.Empty;
	}
}
