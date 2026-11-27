using System;

namespace Voxear.Testing;

/// <summary>What an <see cref="Assert"/> throws when what it checks does not hold: the test fails with its message.</summary>
public sealed class AssertionException(string message) : Exception(message);
