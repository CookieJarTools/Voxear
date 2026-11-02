using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Numerics;
using System.Reflection;
using System.Text;

namespace Voxear.Scripting;

/// <summary>
/// A development build's console, as far as scripts go: a typed line runs a [Command] method by its name with what
/// follows as its arguments, or reads or sets a [ConsoleVariable]. Names match ignoring case. What a line does is
/// handed back as lines to print, so the console shows them as it shows the log.
/// </summary>
internal static class DevConsole
{
	private const BindingFlags Static = BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic;

	/// <summary>Runs a line against the game's scripts. Null when no command or variable has its first word as a name.</summary>
	public static List<(LogLevel Level, string Text)>? Run(string line)
	{
		return Run(line, GameScripts.Assemblies);
	}

	public static List<(LogLevel Level, string Text)>? Run(string line, IEnumerable<Assembly> assemblies)
	{
		var words = Words(line);
		if (words.Count == 0)
		{
			return new List<(LogLevel, string)>();
		}

		var (name, given) = (words[0], words.Skip(1).ToList());
		var printed = new List<(LogLevel Level, string Text)>();
		var scripts = assemblies.ToList();
		if (name.Equals("help", StringComparison.OrdinalIgnoreCase))
		{
			printed.AddRange(Describe(scripts).Select(entry => (LogLevel.Info, entry.Usage + (entry.Help.Length > 0 ? $"  {entry.Help}" : string.Empty))));
			return printed;
		}
		if (Find(Commands(scripts), name) is { } command)
		{
			RunCommand(command, given, printed);
			return printed;
		}
		if (Find(Variables(scripts), name) is { } variable)
		{
			Variable(variable, given, printed);
			return printed;
		}
		return null;
	}

	/// <summary>Every command and variable, as "name\tusage\thelp" lines, for completing names and for help.</summary>
	public static string Names(IEnumerable<Assembly> assemblies)
	{
		var text = new StringBuilder();
		foreach (var (name, usage, help) in Describe(assemblies.ToList()))
		{
			text.Append(name).Append('\t').Append(usage).Append('\t').Append(help).Append('\n');
		}
		return text.ToString();
	}

	/// <summary>The words of a line, split at spaces, with "quoted text" kept as one.</summary>
	public static List<string> Words(string line)
	{
		var words = new List<string>();
		var word = new StringBuilder();
		var quoted = false;
		foreach (var c in line)
		{
			if (c == '"')
			{
				quoted = !quoted;
				continue;
			}
			if (char.IsWhiteSpace(c) && !quoted)
			{
				Flush(word, words);
				continue;
			}
			word.Append(c);
		}
		Flush(word, words);
		return words;
	}

	private static void Flush(StringBuilder word, List<string> words)
	{
		if (word.Length > 0)
		{
			words.Add(word.ToString());
			word.Clear();
		}
	}

	private static List<(string Name, string Usage, string Help)> Describe(List<Assembly> assemblies)
	{
		var commands = Commands(assemblies).Select(method =>
		{
			var parameters = method.GetParameters().Select(p => p.HasDefaultValue ? $"[{p.Name}]" : p.Name);
			var usage = string.Join(" ", new[] { method.Name }.Concat(parameters!));
			return (method.Name, usage, method.GetCustomAttribute<CommandAttribute>()!.Title);
		});
		var variables = Variables(assemblies).Select(member =>
			(member.Name, $"{member.Name} = {Read(member)}", member.GetCustomAttribute<ConsoleVariableAttribute>()!.Help));
		return commands.Concat(variables).OrderBy(entry => entry.Item1, StringComparer.OrdinalIgnoreCase).ToList();
	}

	private static IEnumerable<MethodInfo> Commands(List<Assembly> assemblies)
	{
		return Types(assemblies).SelectMany(type => type.GetMethods(Static | BindingFlags.DeclaredOnly))
			.Where(method => method.IsDefined(typeof(CommandAttribute)) && !method.ContainsGenericParameters);
	}

	private static IEnumerable<MemberInfo> Variables(List<Assembly> assemblies)
	{
		return Types(assemblies).SelectMany(type => type.GetMembers(Static | BindingFlags.DeclaredOnly))
			.Where(member => member is FieldInfo or PropertyInfo && member.IsDefined(typeof(ConsoleVariableAttribute)));
	}

	private static IEnumerable<Type> Types(List<Assembly> assemblies)
	{
		return assemblies.SelectMany(Loadable).OfType<Type>();
	}

	/// <summary>An assembly's types, leaving out any that cannot load rather than failing for all of them.</summary>
	private static Type?[] Loadable(Assembly assembly)
	{
		try
		{
			return assembly.GetTypes();
		}
		catch (ReflectionTypeLoadException exception)
		{
			return exception.Types;
		}
	}

	/// <summary>The one named so, by its own name or as Type.Name for when two types share one.</summary>
	private static T? Find<T>(IEnumerable<T> members, string name) where T : MemberInfo
	{
		return members.FirstOrDefault(m => m.Name.Equals(name, StringComparison.OrdinalIgnoreCase)
			|| $"{m.DeclaringType!.Name}.{m.Name}".Equals(name, StringComparison.OrdinalIgnoreCase));
	}

	private static void RunCommand(MethodInfo method, List<string> given, List<(LogLevel, string)> printed)
	{
		var parameters = method.GetParameters();
		if (given.Count > parameters.Length || given.Count < parameters.Count(p => !p.HasDefaultValue))
		{
			printed.Add((LogLevel.Warning, $"{method.Name} takes {Usage(method)}"));
			return;
		}

		var arguments = new object?[parameters.Length];
		for (var i = 0; i < parameters.Length; i++)
		{
			if (i >= given.Count)
			{
				arguments[i] = parameters[i].DefaultValue;
			}
			else if (!TryParse(given[i], parameters[i].ParameterType, out arguments[i]))
			{
				printed.Add((LogLevel.Warning, $"\"{given[i]}\" is not {Kind(parameters[i].ParameterType)}, as {parameters[i].Name} must be"));
				return;
			}
		}

		object? result;
		try
		{
			result = method.Invoke(null, arguments);
		}
		catch (TargetInvocationException exception) when (exception.InnerException != null)
		{
			printed.Add((LogLevel.Error, $"{method.Name} threw {exception.InnerException.GetType().Name}: {exception.InnerException.Message}"));
			return;
		}
		if (method.ReturnType != typeof(void) && result != null)
		{
			printed.Add((LogLevel.Info, Show(result)));
		}
	}

	private static void Variable(MemberInfo member, List<string> given, List<(LogLevel, string)> printed)
	{
		var type = member is FieldInfo field ? field.FieldType : ((PropertyInfo)member).PropertyType;
		if (given.Count == 0)
		{
			printed.Add((LogLevel.Info, $"{member.Name} = {Read(member)}"));
			return;
		}
		var text = string.Join(" ", given);
		if (!TryParse(text, type, out var value))
		{
			printed.Add((LogLevel.Warning, $"\"{text}\" is not {Kind(type)}, as {member.Name} is"));
			return;
		}
		if (member is FieldInfo writable && !writable.IsInitOnly && !writable.IsLiteral)
		{
			writable.SetValue(null, value);
		}
		else if (member is PropertyInfo property && property.CanWrite)
		{
			property.SetValue(null, value);
		}
		else
		{
			printed.Add((LogLevel.Warning, $"{member.Name} can only be read"));
			return;
		}
		printed.Add((LogLevel.Info, $"{member.Name} = {Read(member)}"));
	}

	private static string Read(MemberInfo member)
	{
		var value = member is FieldInfo field ? field.GetValue(null) : ((PropertyInfo)member).GetValue(null);
		return value == null ? "null" : Show(value);
	}

	private static string Usage(MethodInfo method)
	{
		var parameters = method.GetParameters();
		if (parameters.Length == 0)
		{
			return "nothing";
		}
		return string.Join(", ", parameters.Select(p => $"{p.Name} ({Kind(p.ParameterType)}{(p.HasDefaultValue ? ", optional" : string.Empty)})"));
	}

	/// <summary>A value as the console prints it: numbers the same in every language, and vectors as "x,y,z".</summary>
	private static string Show(object value)
	{
		return value switch
		{
			Vector2 v => string.Create(CultureInfo.InvariantCulture, $"{v.X},{v.Y}"),
			Vector3 v => string.Create(CultureInfo.InvariantCulture, $"{v.X},{v.Y},{v.Z}"),
			IFormattable formattable => formattable.ToString(null, CultureInfo.InvariantCulture),
			_ => value.ToString() ?? string.Empty,
		};
	}

	private static string Kind(Type type)
	{
		if (type.IsEnum)
		{
			return "one of " + string.Join(", ", Enum.GetNames(type));
		}
		if (type == typeof(bool))
		{
			return "true or false";
		}
		if (type == typeof(Vector2) || type == typeof(Vector3))
		{
			return type == typeof(Vector2) ? "a vector as x,y" : "a vector as x,y,z";
		}
		return type == typeof(string) ? "text" : FieldValues.IsInteger(type) ? "a whole number" : "a number";
	}

	public static bool TryParse(string text, Type type, out object? value)
	{
		value = null;
		try
		{
			value = Parse(text, type);
			return value != null;
		}
		catch (Exception exception) when (exception is FormatException or OverflowException or ArgumentException or InvalidCastException)
		{
			return false;
		}
	}

	private static object? Parse(string text, Type type)
	{
		if (type == typeof(string))
		{
			return text;
		}
		if (type == typeof(bool))
		{
			return text.ToLowerInvariant() switch
			{
				"true" or "on" or "yes" or "1" => true,
				"false" or "off" or "no" or "0" => false,
				_ => null,
			};
		}
		if (type.IsEnum)
		{
			return Enum.Parse(type, text, ignoreCase: true);
		}
		if (type == typeof(Vector2) || type == typeof(Vector3))
		{
			var parts = text.Split(',').Select(part => float.Parse(part, CultureInfo.InvariantCulture)).ToArray();
			return parts.Length switch
			{
				2 when type == typeof(Vector2) => new Vector2(parts[0], parts[1]),
				3 when type == typeof(Vector3) => new Vector3(parts[0], parts[1], parts[2]),
				_ => null,
			};
		}
		return type.IsPrimitive || type == typeof(decimal) ? Convert.ChangeType(text, type, CultureInfo.InvariantCulture) : null;
	}
}
