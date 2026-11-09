using System;
using System.Collections.Generic;

namespace Voxear.Analyzers;

/// <summary>
/// Reads the actions of an .inputactions file, which is TOML, as far as Actions needs: each [[map]]'s name, and each
/// of its [[map.action]] tables' name and id. Everything else, bindings and categories among it, is passed over.
/// </summary>
internal static class InputActionsReader
{
	public static List<InputActionEntry> Read(string text)
	{
		var found = new List<InputActionEntry>();
		var section = string.Empty;
		var map = string.Empty;
		string? name = null;
		string? id = null;
		foreach (var raw in text.Split('\n'))
		{
			var line = raw.Trim();
			if (line.StartsWith("[", StringComparison.Ordinal))
			{
				Add(found, section, map, name, id);
				(section, name, id) = (line, null, null);
				continue;
			}

			var value = Value(line, "name");
			if (value != null && section == "[[map]]")
			{
				map = value;
			}
			else if (value != null && section == "[[map.action]]")
			{
				name = value;
			}
			id = section == "[[map.action]]" ? Value(line, "id") ?? id : id;
		}
		Add(found, section, map, name, id);
		return found;
	}

	private static void Add(List<InputActionEntry> found, string section, string map, string? name, string? id)
	{
		if (section == "[[map.action]]" && name != null)
		{
			found.Add(new InputActionEntry(map, name, string.IsNullOrEmpty(id) ? $"{map}/{name}" : id!));
		}
	}

	/// <summary>The basic string a line gives the key, as <c>name = "Jump"</c>, or null.</summary>
	private static string? Value(string line, string key)
	{
		if (!line.StartsWith(key, StringComparison.Ordinal))
		{
			return null;
		}
		var rest = line.Substring(key.Length).TrimStart();
		if (!rest.StartsWith("=", StringComparison.Ordinal))
		{
			return null;
		}
		rest = rest.Substring(1).Trim();
		if (rest.Length < 2 || rest[0] != '"')
		{
			return null;
		}
		var end = rest.IndexOf('"', 1);
		return end > 0 ? rest.Substring(1, end - 1).Replace("\\\\", "\\") : null;
	}
}
