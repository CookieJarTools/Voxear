using System;
using System.Collections.Generic;
using System.Numerics;

namespace Voxear.Testing;

/// <summary>
/// What a test checks. Each throws an <see cref="AssertionException"/> when what it checks does not hold, which fails
/// the test with what was expected and what was found, after the message given.
/// </summary>
public static class Assert
{
	/// <summary>Fails unless the condition holds.</summary>
	public static void True(bool condition, string message = "")
	{
		if (!condition)
		{
			Fail(Said(message, "expected true"));
		}
	}

	/// <summary>Fails if the condition holds.</summary>
	public static void False(bool condition, string message = "")
	{
		if (condition)
		{
			Fail(Said(message, "expected false"));
		}
	}

	/// <summary>Fails unless the two are equal.</summary>
	public static void Equal<T>(T expected, T actual, string message = "")
	{
		if (!EqualityComparer<T>.Default.Equals(expected, actual))
		{
			Fail(Said(message, $"expected {expected}, found {actual}"));
		}
	}

	/// <summary>Fails if the two are equal.</summary>
	public static void NotEqual<T>(T unexpected, T actual, string message = "")
	{
		if (EqualityComparer<T>.Default.Equals(unexpected, actual))
		{
			Fail(Said(message, $"expected anything but {unexpected}"));
		}
	}

	/// <summary>Fails unless the two numbers are within tolerance of each other.</summary>
	public static void Near(float expected, float actual, float tolerance = 1e-4f, string message = "")
	{
		if (!(MathF.Abs(expected - actual) <= tolerance))
		{
			Fail(Said(message, $"expected {expected} ± {tolerance}, found {actual}"));
		}
	}

	/// <summary>Fails unless the two vectors are within tolerance of each other.</summary>
	public static void Near(Vector3 expected, Vector3 actual, float tolerance = 1e-4f, string message = "")
	{
		if (!(Vector3.Distance(expected, actual) <= tolerance))
		{
			Fail(Said(message, $"expected {expected} ± {tolerance}, found {actual}"));
		}
	}

	/// <summary>Fails if the value is null.</summary>
	public static void NotNull(object? value, string message = "")
	{
		if (value is null)
		{
			Fail(Said(message, "expected something, found null"));
		}
	}

	/// <summary>Fails unless the value is null.</summary>
	public static void Null(object? value, string message = "")
	{
		if (value is not null)
		{
			Fail(Said(message, $"expected null, found {value}"));
		}
	}

	/// <summary>Fails unless running the action throws an exception of type T, and gives back what it threw.</summary>
	public static T Throws<T>(Action action, string message = "") where T : Exception
	{
		try
		{
			action();
		}
		catch (T thrown)
		{
			return thrown;
		}
		catch (Exception other)
		{
			Fail(Said(message, $"expected {typeof(T).Name}, it threw {other.GetType().Name}: {other.Message}"));
		}
		Fail(Said(message, $"expected {typeof(T).Name}, nothing was thrown"));
		return null!;
	}

	/// <summary>Fails the test, saying why.</summary>
	public static void Fail(string message)
	{
		throw new AssertionException(message);
	}

	private static string Said(string message, string found) => message.Length == 0 ? found : $"{message}: {found}";
}
