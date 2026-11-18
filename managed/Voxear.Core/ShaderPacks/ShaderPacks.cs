using System;
using System.Collections.Generic;
using System.Text.Json;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The shader packs a player can wear, the one they wear and its options. A pack changes how every level is lit and
/// finished, so the choice goes with the player from level to level, and it is kept beside the saves with the
/// options they set, so the game starts in it again. A pack compiles on a thread the first time it is worn, which
/// takes a few seconds; the compiled programs are kept beside the saves too, so after that it is worn at once.
/// <code>
/// ShaderPacks.Use(ShaderPack.Evening);
/// ShaderPacks.SetOption("warmth", 0.9f);
/// </code>
/// <para>
/// All lists the engine's own packs, the project's <c>.shaderpack</c> folders, and any the player put in the game's
/// ShaderPacks folder (Folder). ShaderPackScreen is a ready-made menu of them and of the options of the one worn.
/// </para>
/// </summary>
public static class ShaderPacks
{
	/// <summary>The pack worn changed, by Use or UseNone: the one worn now, or ShaderPack.None.</summary>
	public static event Action<ShaderPack>? Changed;

	/// <summary>
	/// Every pack there is to wear: the engine's own, then the game's, then the player's, each by name. Read afresh
	/// each time, so a folder the player just added is there.
	/// </summary>
	public static IReadOnlyList<ShaderPack> All => Read().Packs;

	/// <summary>The pack worn, or being put on; ShaderPack.None for none.</summary>
	public static ShaderPack Current => Read().Worn;

	/// <summary>How the pack worn stands: compiling, drawn, or failed to compile.</summary>
	public static ShaderPackStanding Standing => Read().Standing;

	/// <summary>Why the pack worn did not compile, with the file and line; empty when it did.</summary>
	public static string Error => Read().Error;

	/// <summary>The options of the pack worn, in its own order, each with its value now; empty for none.</summary>
	public static IReadOnlyList<ShaderPackOption> Options => Read().Options;

	/// <summary>
	/// The folder a player puts <c>.shaderpack</c> folders of their own in, beside the game's saves; null where
	/// there is nowhere to keep them.
	/// </summary>
	public static string? Folder => Read().Folder;

	/// <summary>
	/// Wears a pack from the next frame, over every level until another is worn: compiled on a thread the first time,
	/// the pack worn before drawn until it is ready. The choice is kept for the game's next run unless
	/// <paramref name="remember"/> is false, as for a preview. Options the pack shares with the one worn before, by
	/// name, keep their values. A pack that cannot be read is reported in the Console, and the one worn stays.
	/// </summary>
	public static void Use(ShaderPack pack, bool remember = true)
	{
		if (!pack.IsSet)
		{
			UseNone(remember);
			return;
		}

		NativeShaderPacks.Wear(pack.Reference, remember);
		Changed?.Invoke(pack);
	}

	/// <summary>Takes off the pack worn: every level is drawn as the engine draws it.</summary>
	public static void UseNone(bool remember = true)
	{
		NativeShaderPacks.Wear(string.Empty, remember);
		Changed?.Invoke(ShaderPack.None);
	}

	/// <summary>
	/// Sets a number option of the pack worn, by its name, after the frame, kept between its least and greatest; and
	/// keeps it with the pack for the game's next run unless <paramref name="remember"/> is false.
	/// </summary>
	public static void SetOption(string name, float value, bool remember = true)
	{
		NativeShaderPacks.SetOption(name, value, remember);
	}

	/// <summary>Turns a toggle option of the pack worn on or off, by its name.</summary>
	public static void SetOption(string name, bool on, bool remember = true)
	{
		NativeShaderPacks.SetOption(name, on ? 1f : 0f, remember);
	}

	/// <summary>Picks a choice of a choice option of the pack worn, both by name. An unknown choice is reported.</summary>
	public static void SetOption(string name, string choice, bool remember = true)
	{
		foreach (var option in Options)
		{
			if (option.Name != name)
			{
				continue;
			}

			var at = -1;
			for (var i = 0; i < option.Choices.Count; i++)
			{
				if (string.Equals(option.Choices[i], choice, StringComparison.OrdinalIgnoreCase))
				{
					at = i;
				}
			}

			if (at < 0)
			{
				Log.Warning($"the shader pack's option {name} has no choice {choice}");
				return;
			}

			NativeShaderPacks.SetOption(name, at, remember);
			return;
		}

		Log.Warning($"the shader pack worn has no option {name}");
	}

	private static ShaderPacksRead Read()
	{
		var read = new ShaderPacksRead();
		var json = NativeShaderPacks.Info();
		if (json == null)
		{
			return read;
		}

		using var document = JsonDocument.Parse(json);
		var root = document.RootElement;
		foreach (var entry in root.GetProperty("packs").EnumerateArray())
		{
			var source = entry.GetProperty("source").GetString() switch
			{
				"game" => ShaderPackSource.Game,
				"player" => ShaderPackSource.Player,
				_ => ShaderPackSource.BuiltIn,
			};
			read.Packs.Add(new ShaderPack(Text(entry, "reference"), Text(entry, "name"), Text(entry, "description"), source));
		}

		read.Folder = root.GetProperty("folder").GetString();
		var worn = root.GetProperty("worn");
		if (worn.ValueKind != JsonValueKind.Object)
		{
			return read;
		}

		var named = Text(worn, "reference");
		var listed = read.Packs.FindIndex(pack => pack.Reference == named);
		read.Worn = listed >= 0 ? read.Packs[listed] : ShaderPack.Load(named);
		if (worn.TryGetProperty("standing", out var standing) && standing.ValueKind == JsonValueKind.Object)
		{
			read.Standing = Text(standing, "state") switch
			{
				"compiling" => ShaderPackStanding.Compiling,
				"failed" => ShaderPackStanding.Failed,
				_ => ShaderPackStanding.Drawn,
			};
			read.Error = standing.TryGetProperty("error", out var error) ? error.GetString() ?? string.Empty : string.Empty;
		}

		foreach (var option in worn.GetProperty("options").EnumerateArray())
		{
			read.Options.Add(OptionOf(option));
		}

		return read;
	}

	private static ShaderPackOption OptionOf(JsonElement option)
	{
		var (name, label, about) = (Text(option, "key"), Text(option, "label"), Text(option, "about"));
		var value = option.GetProperty("value");
		switch (Text(option, "kind"))
		{
			case "toggle":
				var on = value.ValueKind == JsonValueKind.True;
				return new ShaderPackOption(name, label, about, ShaderPackOptionKind.Toggle, (0f, 1f), [], on ? 1f : 0f);
			case "choice":
				var choices = new List<string>();
				foreach (var choice in option.GetProperty("choices").EnumerateArray())
				{
					choices.Add(choice.GetString() ?? string.Empty);
				}
				var picked = Math.Max(choices.IndexOf(value.GetString() ?? string.Empty), 0);
				var range = (0f, Math.Max(choices.Count - 1, 0));
				return new ShaderPackOption(name, label, about, ShaderPackOptionKind.Choice, range, choices, picked);
			default:
				var (low, high) = (option.GetProperty("low").GetSingle(), option.GetProperty("high").GetSingle());
				var number = value.GetSingle();
				return new ShaderPackOption(name, label, about, ShaderPackOptionKind.Number, (low, high), [], number);
		}
	}

	private static string Text(JsonElement entry, string name)
	{
		return entry.TryGetProperty(name, out var value) ? value.GetString() ?? string.Empty : string.Empty;
	}
}
