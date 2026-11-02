using System;

namespace Voxear;

/// <summary>
/// Shows part of a VoxDocs page above a field in the Inspector, in a fold that starts closed. Where
/// <see cref="DocumentationAttribute"/> offers a link to go and read something, this puts the words beside the thing
/// they are about.
/// <para>
/// The page is named as for [Documentation]: by its path or its id. A heading after a hash takes that section
/// rather than the whole page, <c>"Combat/Damage#falloff"</c>: it runs from the heading down to the next heading as
/// big or bigger. The Inspector follows the page, so editing it shows straight away.
/// </para>
/// </summary>
[AttributeUsage(AttributeTargets.Field, Inherited = true)]
public sealed class ExcerptAttribute : Attribute
{
	public ExcerptAttribute(string page)
	{
		Page = page;
	}

	/// <summary>The page, as its id or its path, and a heading after a hash if any.</summary>
	public string Page { get; }
}
