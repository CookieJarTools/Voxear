using System;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;

namespace Voxear.ScriptInfo;

/// <summary>
/// Types and members written as C# declares them, <c>public static Prefab Load(string path)</c>, for the Code
/// Reference's pages. Reflection names types the runtime's way, <c>Ref`1</c> and <c>System.Int32</c>, which
/// nobody writes.
/// </summary>
internal static class Signatures
{
	private static readonly Dictionary<Type, string> Keywords = new Dictionary<Type, string>
	{
		{ typeof(void), "void" },
		{ typeof(bool), "bool" },
		{ typeof(byte), "byte" },
		{ typeof(sbyte), "sbyte" },
		{ typeof(char), "char" },
		{ typeof(short), "short" },
		{ typeof(ushort), "ushort" },
		{ typeof(int), "int" },
		{ typeof(uint), "uint" },
		{ typeof(long), "long" },
		{ typeof(ulong), "ulong" },
		{ typeof(float), "float" },
		{ typeof(double), "double" },
		{ typeof(decimal), "decimal" },
		{ typeof(string), "string" },
		{ typeof(object), "object" },
	};

	/// <summary>A type as code writes it: <c>int</c>, <c>Ref&lt;Prefab&gt;</c>, <c>float[]</c>, <c>int?</c>.</summary>
	public static string Name(Type type)
	{
		if (Keywords.TryGetValue(type, out var keyword))
		{
			return keyword;
		}

		if (type.IsByRef || type.IsPointer)
		{
			return Name(type.GetElementType()!);
		}

		if (type.IsArray)
		{
			return Name(type.GetElementType()!) + "[" + new string(',', type.GetArrayRank() - 1) + "]";
		}

		var nullable = Nullable.GetUnderlyingType(type);
		if (nullable != null)
		{
			return Name(nullable) + "?";
		}

		return Bare(type) + Arguments(type);
	}

	/// <summary>A type's own name and its generic parameters, as its page is titled: <c>Ref&lt;T&gt;</c>.</summary>
	public static string Title(Type type)
	{
		return Bare(type) + Arguments(type);
	}

	/// <summary>How a type is declared: <c>public sealed class Prefab : Asset</c>.</summary>
	public static string Declaration(Type type)
	{
		var kind = Kind(type);
		var modifiers = kind == "class" && type.IsAbstract && type.IsSealed ? "static "
			: kind == "class" && type.IsAbstract ? "abstract "
			: kind == "class" && type.IsSealed ? "sealed "
			: string.Empty;
		var bases = new List<string>();
		if (kind == "class" && type.BaseType != null && type.BaseType != typeof(object))
		{
			bases.Add(Name(type.BaseType));
		}

		bases.AddRange(type.GetInterfaces().Where(i => i.IsPublic).Select(Name));
		var after = bases.Count > 0 && kind != "enum" ? " : " + string.Join(", ", bases) : string.Empty;
		return "public " + modifiers + kind + " " + Title(type) + after;
	}

	/// <summary>What a type is: class, struct, interface, enum or delegate.</summary>
	public static string Kind(Type type)
	{
		if (type.IsEnum)
		{
			return "enum";
		}

		if (type.IsInterface)
		{
			return "interface";
		}

		if (type.IsValueType)
		{
			return "struct";
		}

		return typeof(Delegate).IsAssignableFrom(type) ? "delegate" : "class";
	}

	/// <summary>How a member is declared, as its line on a type's page shows it.</summary>
	public static string Declaration(MemberInfo member)
	{
		switch (member)
		{
			case ConstructorInfo constructor:
				return Access(constructor) + Bare(constructor.DeclaringType!) + Parameters(constructor.GetParameters());
			case MethodInfo method:
				return Access(method) + Static(method.IsStatic) + Overriding(method) + Name(method.ReturnType) + " " + method.Name + MethodArguments(method) + Parameters(method.GetParameters());
			case PropertyInfo property:
				return Property(property);
			case FieldInfo field:
				return Field(field);
			case EventInfo @event:
				return "public event " + Name(@event.EventHandlerType!) + " " + @event.Name;
			default:
				return member.Name;
		}
	}

	/// <summary>A member's short name, as its heading: <c>Load(string)</c> for a method.</summary>
	public static string Heading(MemberInfo member)
	{
		switch (member)
		{
			case ConstructorInfo constructor:
				return Bare(constructor.DeclaringType!) + "(" + string.Join(", ", constructor.GetParameters().Select(p => Name(p.ParameterType))) + ")";
			case MethodInfo method:
				return method.Name + MethodArguments(method) + "(" + string.Join(", ", method.GetParameters().Select(p => Name(p.ParameterType))) + ")";
			default:
				return member.Name;
		}
	}

	private static string Property(PropertyInfo property)
	{
		var get = property.GetMethod;
		var set = property.SetMethod;
		var shown = get != null && (get.IsPublic || get.IsFamily) ? get : set!;
		var accessors = "{ " + (get != null && get.IsPublic ? "get; " : string.Empty) + (set != null && set.IsPublic ? "set; " : string.Empty) + "}";
		return Access(shown) + Static(shown.IsStatic) + Overriding(shown) + Name(property.PropertyType) + " " + property.Name + " " + accessors;
	}

	private static string Field(FieldInfo field)
	{
		if (field.IsLiteral && field.DeclaringType!.IsEnum)
		{
			return field.Name + " = " + Convert.ToInt64(field.GetRawConstantValue());
		}

		var constant = field.IsLiteral ? "const " : (field.IsStatic ? "static " : string.Empty) + (field.IsInitOnly ? "readonly " : string.Empty);
		var access = field.IsPublic ? "public " : field.IsFamily ? "protected " : "[SerializeField] private ";
		return access + constant + Name(field.FieldType) + " " + field.Name;
	}

	private static string Access(MethodBase method)
	{
		return method.IsPublic ? "public " : "protected ";
	}

	/// <summary>
	/// Whether a member can be or is overridden, which says how a behaviour uses it: <c>protected virtual void
	/// Update()</c> is there to override. An interface's members, and a class's that implement them, say nothing.
	/// </summary>
	private static string Overriding(MethodInfo method)
	{
		if (method.DeclaringType!.IsInterface || !method.IsVirtual)
		{
			return string.Empty;
		}

		if (method.IsAbstract)
		{
			return "abstract ";
		}

		var overrides = method.GetBaseDefinition() != method;
		if (!overrides)
		{
			return method.IsFinal ? string.Empty : "virtual ";
		}

		return method.IsFinal ? "sealed override " : "override ";
	}

	private static string Static(bool isStatic)
	{
		return isStatic ? "static " : string.Empty;
	}

	private static string Bare(Type type)
	{
		var tick = type.Name.IndexOf('`');
		return tick < 0 ? type.Name : type.Name.Substring(0, tick);
	}

	private static string Arguments(Type type)
	{
		if (!type.IsGenericType)
		{
			return string.Empty;
		}

		// A nested type's arguments include its declaring type's, which are written there.
		var own = type.GetGenericArguments().Skip(type.DeclaringType?.GetGenericArguments().Length ?? 0).ToArray();
		return own.Length == 0 ? string.Empty : "<" + string.Join(", ", own.Select(Name)) + ">";
	}

	private static string MethodArguments(MethodInfo method)
	{
		return method.IsGenericMethodDefinition ? "<" + string.Join(", ", method.GetGenericArguments().Select(Name)) + ">" : string.Empty;
	}

	private static string Parameters(ParameterInfo[] parameters)
	{
		return "(" + string.Join(", ", parameters.Select(Parameter)) + ")";
	}

	private static string Parameter(ParameterInfo parameter)
	{
		var type = parameter.ParameterType;
		var mode = !type.IsByRef ? string.Empty : parameter.IsOut ? "out " : parameter.IsIn ? "in " : "ref ";
		var extra = parameter.IsDefined(typeof(ParamArrayAttribute)) ? "params " : string.Empty;
		return extra + mode + Name(type) + " " + parameter.Name;
	}
}
