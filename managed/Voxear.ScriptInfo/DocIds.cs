using System;
using System.Linq;
using System.Reflection;

namespace Voxear.ScriptInfo
{
	/// <summary>
	/// The names the C# compiler gives members in its XML documentation, such as
	/// <c>M:Voxear.Prefab.Load(System.String)</c>, made from reflection so a member's comments can be found.
	/// Generic parameters are written by position, two backticks for a method's and one for its type's.
	/// </summary>
	internal static class DocIds
	{
		public static string Of(MemberInfo member)
		{
			switch (member)
			{
				case Type type:
					return "T:" + TypeName(type);
				case ConstructorInfo constructor:
					return "M:" + TypeName(constructor.DeclaringType!) + ".#ctor" + Parameters(constructor.GetParameters());
				case MethodInfo method:
					return "M:" + TypeName(method.DeclaringType!) + "." + method.Name + Arity(method) + Parameters(method.GetParameters());
				case PropertyInfo property:
					return "P:" + TypeName(property.DeclaringType!) + "." + property.Name + Parameters(property.GetIndexParameters());
				case FieldInfo field:
					return "F:" + TypeName(field.DeclaringType!) + "." + field.Name;
				case EventInfo @event:
					return "E:" + TypeName(@event.DeclaringType!) + "." + @event.Name;
				default:
					return member.Name;
			}
		}

		/// <summary>A type as the documentation names it where it is declared: nested types joined by dots.</summary>
		public static string TypeName(Type type)
		{
			return (type.FullName ?? type.Name).Replace('+', '.');
		}

		private static string Arity(MethodInfo method)
		{
			return method.IsGenericMethodDefinition ? "``" + method.GetGenericArguments().Length : string.Empty;
		}

		private static string Parameters(ParameterInfo[] parameters)
		{
			if (parameters.Length == 0)
			{
				return string.Empty;
			}

			return "(" + string.Join(",", parameters.Select(p => Used(p.ParameterType))) + ")";
		}

		/// <summary>A type as the documentation names it where it is used, in a parameter list.</summary>
		private static string Used(Type type)
		{
			if (type.IsByRef)
			{
				return Used(type.GetElementType()!) + "@";
			}

			if (type.IsArray)
			{
				return Used(type.GetElementType()!) + "[]";
			}

			if (type.IsGenericParameter)
			{
				return (type.DeclaringMethod != null ? "``" : "`") + type.GenericParameterPosition;
			}

			if (!type.IsGenericType)
			{
				return TypeName(type);
			}

			var name = TypeName(type.GetGenericTypeDefinition());
			var tick = name.IndexOf('`');
			var bare = tick < 0 ? name : name.Substring(0, tick);
			return bare + "{" + string.Join(",", type.GetGenericArguments().Select(Used)) + "}";
		}
	}
}
