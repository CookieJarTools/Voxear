using System;
using System.Collections.Generic;
using System.Numerics;
using System.Reflection;
using System.Text;
using System.Text.Json;
using Voxear.Saving;

namespace Voxear.Scripting;

/// <summary>
/// The game's [Call] methods, as tools make them: listed for the editor's catalog with their arguments, and run with
/// arguments read from JSON and what they return written back as JSON, as saves read and write values.
/// </summary>
internal static class GameCalls
{
	private const BindingFlags Static = BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.DeclaredOnly;

	/// <summary>What became of a call.</summary>
	public enum Ran
	{
		Done,
		NoSuchCall,
		BadArguments,
	}

	/// <summary>Each [Call] method of the game's types, with the call it is.</summary>
	public static IEnumerable<(Type Type, MethodInfo Method, CallAttribute Call)> In(IEnumerable<Type?> types)
	{
		foreach (var type in types)
		{
			if (type == null)
			{
				continue;
			}
			foreach (var method in type.GetMethods(Static))
			{
				if (method.GetCustomAttribute<CallAttribute>() is { } call && !method.ContainsGenericParameters)
				{
					yield return (type, method, call);
				}
			}
		}
	}

	/// <summary>The kind of value a tool gives for a parameter of this type, as the editor's calls name kinds.</summary>
	public static string KindOf(Type type)
	{
		var plain = Nullable.GetUnderlyingType(type) ?? type;
		return plain switch
		{
			_ when plain == typeof(bool) => "toggle",
			_ when plain == typeof(string) => "text",
			_ when plain == typeof(float) || plain == typeof(double) || plain == typeof(decimal) => "number",
			_ when plain.IsPrimitive && plain != typeof(char) => "whole",
			_ when plain.IsEnum => "choice",
			_ when plain == typeof(Vector3) => "vector",
			_ when plain == typeof(Obj) => "obj",
			_ => "value",
		};
	}

	/// <summary>
	/// Runs the [Call] method <paramref name="methodName"/> of <paramref name="typeName"/> with the arguments of the
	/// JSON object <paramref name="arguments"/>, by name; <paramref name="answer"/> is what it returned as JSON, or
	/// what was wrong with the arguments. What the method throws reaches the caller.
	/// </summary>
	public static Ran Run(string typeName, string methodName, string arguments, out string answer)
	{
		answer = string.Empty;
		MethodInfo? found = null;
		foreach (var assembly in GameScripts.Assemblies)
		{
			found = assembly.GetType(typeName)?.GetMethod(methodName, Static);
			if (found?.GetCustomAttribute<CallAttribute>() != null)
			{
				break;
			}
			found = null;
		}
		if (found == null)
		{
			return Ran.NoSuchCall;
		}

		using var document = JsonDocument.Parse(string.IsNullOrWhiteSpace(arguments) ? "{}" : arguments);
		var given = document.RootElement;
		var parameters = found.GetParameters();
		var values = new object?[parameters.Length];
		for (var at = 0; at < parameters.Length; at++)
		{
			var parameter = parameters[at];
			var name = parameter.Name ?? string.Empty;
			if (given.ValueKind != JsonValueKind.Object || !given.TryGetProperty(name, out var value) || value.ValueKind == JsonValueKind.Null)
			{
				if (!parameter.HasDefaultValue)
				{
					answer = $"`{name}` is missing";
					return Ran.BadArguments;
				}
				values[at] = parameter.DefaultValue;
				continue;
			}
			try
			{
				values[at] = Read(value, parameter.ParameterType);
			}
			catch (Exception exception) when (exception is FormatException or JsonException or InvalidOperationException or ArgumentException or SaveException)
			{
				answer = $"`{name}` is not a {parameter.ParameterType.Name}: {exception.Message}";
				return Ran.BadArguments;
			}
		}

		object? returned;
		try
		{
			returned = found.Invoke(null, values);
		}
		catch (TargetInvocationException exception) when (exception.InnerException != null)
		{
			System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(exception.InnerException).Throw();
			throw;
		}
		answer = found.ReturnType == typeof(void)
			? "null"
			: Encoding.UTF8.GetString(SaveFormat.Json.Write(SaveWriter.Write(returned, found.ReturnType)));
		return Ran.Done;
	}

	/// <summary>A value a tool gave, as a parameter of <paramref name="type"/> takes it.</summary>
	private static object? Read(JsonElement value, Type type)
	{
		var plain = Nullable.GetUnderlyingType(type) ?? type;
		if (plain == typeof(Obj))
		{
			return new Obj(value.GetUInt64());
		}
		if (plain.IsEnum && value.ValueKind == JsonValueKind.String)
		{
			return Enum.Parse(plain, value.GetString()!, ignoreCase: true);
		}
		var node = SaveFormat.Json.Read(Encoding.UTF8.GetBytes(value.GetRawText()));
		return SaveReader.Read(node, type, null);
	}
}
