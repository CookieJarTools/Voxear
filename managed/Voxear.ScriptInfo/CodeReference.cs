using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Text;
using System.Text.Json;

namespace Voxear.ScriptInfo;

/// <summary>
/// The Code Reference VoxDocs shows: every public type of the game's scripts and of Voxear.Core, with its
/// members, how each is declared, and its <c>///</c> comments as Markdown. Members are the public and protected
/// ones, and the private fields a <c>[SerializeField]</c> puts in the Inspector, since those are part of how a
/// behaviour is used too.
/// </summary>
internal static class CodeReference
{
	private const int Format = 1;
	private const BindingFlags Declared = BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance | BindingFlags.Static | BindingFlags.DeclaredOnly;

	/// <summary>The reference for some assemblies, as JSON.</summary>
	public static string Describe(IReadOnlyList<Assembly> assemblies)
	{
		var types = assemblies.SelectMany(Shown).OrderBy(DocIds.TypeName, StringComparer.Ordinal).ToList();
		var pages = new HashSet<string>(types.Select(DocIds.TypeName));
		var comments = new DocComments(assemblies.Select(a => Path.ChangeExtension(a.Location, ".xml")), pages);
		var folders = assemblies.Distinct().ToDictionary(a => a, a => new SourceFolders(a));
		using var stream = new MemoryStream();
		using (var json = new Utf8JsonWriter(stream, new JsonWriterOptions { Indented = true }))
		{
			json.WriteStartObject();
			json.WriteNumber("format", Format);
			json.WriteStartArray("types");
			foreach (var type in types)
			{
				WriteType(json, type, comments, folders[type.Assembly].Of(type));
			}

			json.WriteEndArray();
			json.WriteEndObject();
		}

		foreach (var folder in folders.Values)
		{
			folder.Dispose();
		}

		return Encoding.UTF8.GetString(stream.ToArray());
	}

	private static IEnumerable<Type> Shown(Assembly assembly)
	{
		// A name nobody could write, as C# 14's extension blocks give the types they compile to, is the compiler's.
		return assembly.GetExportedTypes().Where(t => !t.IsDefined(typeof(CompilerGeneratedAttribute)) && !t.Name.Contains('<'));
	}

	private static void WriteType(Utf8JsonWriter json, Type type, DocComments comments, string folder)
	{
		var id = DocIds.Of(type);
		json.WriteStartObject();
		json.WriteString("key", DocIds.TypeName(type));
		json.WriteString("namespace", type.Namespace ?? string.Empty);
		json.WriteString("name", Signatures.Title(type));
		json.WriteString("kind", Signatures.Kind(type));
		json.WriteString("declaration", Signatures.Declaration(type));
		json.WriteString("assembly", type.Assembly.GetName().Name);
		json.WriteString("folder", folder);
		json.WriteString("summary", comments.Part(id, "summary"));
		json.WriteString("remarks", comments.Part(id, "remarks"));
		json.WriteString("documentation", Page(type));
		json.WriteStartArray("members");
		foreach (var member in Members(type).Where(m => !Routine(m, comments)))
		{
			WriteMember(json, member, comments);
		}

		json.WriteEndArray();
		json.WriteEndObject();
	}

	private static void WriteMember(Utf8JsonWriter json, MemberInfo member, DocComments comments)
	{
		var id = DocIds.Of(Commented(member, comments));
		json.WriteStartObject();
		json.WriteString("kind", MemberKind(member));
		json.WriteString("name", Signatures.Heading(member));
		json.WriteString("declaration", Signatures.Declaration(member));
		json.WriteString("summary", comments.Part(id, "summary"));
		json.WriteString("returns", comments.Part(id, "returns"));
		json.WriteString("documentation", Page(member));
		json.WriteStartArray("parameters");
		foreach (var (name, text) in comments.Parameters(id))
		{
			json.WriteStartObject();
			json.WriteString("name", name);
			json.WriteString("text", text);
			json.WriteEndObject();
		}

		json.WriteEndArray();
		json.WriteEndObject();
	}

	/// <summary>The VoxDocs page a member's [Documentation] or [Excerpt] names, or nothing.</summary>
	private static string Page(MemberInfo member)
	{
		return member.GetCustomAttribute<DocumentationAttribute>(true)?.Page ?? member.GetCustomAttribute<ExcerptAttribute>(true)?.Page ?? string.Empty;
	}

	/// <summary>A type's members as its page lists them, in the order they are declared within each kind.</summary>
	private static IEnumerable<MemberInfo> Members(Type type)
	{
		if (typeof(Delegate).IsAssignableFrom(type))
		{
			return Enumerable.Empty<MemberInfo>();
		}

		return type.GetMembers(Declared).Where(Shown).OrderBy(m => m.MetadataToken);
	}

	private static bool Shown(MemberInfo member)
	{
		switch (member)
		{
			case ConstructorInfo constructor:
				return !constructor.IsStatic && Visible(constructor);
			case MethodInfo method:
				return !method.IsSpecialName && Visible(method);
			case PropertyInfo property:
				return property.GetAccessors(true).Any(Visible);
			case FieldInfo field:
				return !field.IsSpecialName && !field.IsDefined(typeof(CompilerGeneratedAttribute)) && (field.IsPublic || field.IsFamily || field.IsDefined(typeof(SerializeFieldAttribute)));
			case EventInfo @event:
				return @event.AddMethod != null && Visible(@event.AddMethod);
			default:
				return false;
		}
	}

	/// <summary>
	/// The member whose comment a member's section shows: its own, or for an override with none, that of the
	/// method it overrides, which says when the engine calls it. A behaviour's Update needs no comment of its own.
	/// </summary>
	private static MemberInfo Commented(MemberInfo member, DocComments comments)
	{
		if (member is not MethodInfo method || comments.Has(DocIds.Of(method)))
		{
			return member;
		}

		return method.GetBaseDefinition();
	}

	/// <summary>
	/// Members with nothing written about them that nobody looks up: Equals, GetHashCode and ToString, which a
	/// type has so comparing and printing work, and a constructor with no parameters, which every class has
	/// unless it says otherwise, and which a behaviour's page would only mislead with.
	/// </summary>
	private static bool Routine(MemberInfo member, DocComments comments)
	{
		var routine = member is MethodInfo { Name: "Equals" or "GetHashCode" or "ToString" }
			|| member is ConstructorInfo constructor && constructor.GetParameters().Length == 0;
		return routine && comments.Part(DocIds.Of(member), "summary").Length == 0;
	}

	private static bool Visible(MethodBase method)
	{
		return method.IsPublic || method.IsFamily || method.IsFamilyOrAssembly;
	}

	private static string MemberKind(MemberInfo member)
	{
		switch (member)
		{
			case ConstructorInfo:
				return "constructor";
			case MethodInfo:
				return "method";
			case PropertyInfo:
				return "property";
			case FieldInfo field when field.IsLiteral && field.DeclaringType!.IsEnum:
				return "value";
			case FieldInfo:
				return "field";
			default:
				return "event";
		}
	}
}
