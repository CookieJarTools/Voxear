using System;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using System.Runtime.Loader;
using Voxear.Scripting;

namespace Voxear.Saving
{
	/// <summary>
	/// What saves know about types: their converters, the formats there are, each type's version and migrations, and
	/// the names a save writes types by. Converters and formats are found in the game scripts by themselves; the ones
	/// added in code win over those. Everything found is forgotten when the scripts reload, and whatever the old
	/// scripts added is dropped.
	/// </summary>
	internal static class SaveTypes
	{
		private const BindingFlags Statics = BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.DeclaredOnly;

		private static readonly Dictionary<Type, SaveConverter> addedConverters = new Dictionary<Type, SaveConverter>();
		private static readonly Dictionary<string, SaveFormat> addedFormats = new Dictionary<string, SaveFormat>();
		private static readonly Dictionary<Type, SortedDictionary<int, Action<SaveNode>>> addedMigrations =
			new Dictionary<Type, SortedDictionary<int, Action<SaveNode>>>();

		/// <summary>Assemblies searched beside the game scripts, as the tests search their own.</summary>
		private static readonly List<Assembly> searched = new List<Assembly>();

		private static readonly Dictionary<Type, SaveConverter?> converters = new Dictionary<Type, SaveConverter?>();
		private static readonly Dictionary<Type, SortedDictionary<int, Action<SaveNode>>> migrations =
			new Dictionary<Type, SortedDictionary<int, Action<SaveNode>>>();
		private static readonly Dictionary<string, Type?> named = new Dictionary<string, Type?>();
		private static readonly Dictionary<Type, (int Version, bool Marked)> versions = new Dictionary<Type, (int Version, bool Marked)>();
		private static Dictionary<Type, SaveConverter>? foundConverters;
		private static Dictionary<string, SaveFormat>? foundFormats;

		public static SaveConverter? Converter(Type type)
		{
			if (converters.TryGetValue(type, out var known))
			{
				return known;
			}

			FindInScripts();
			var converter = addedConverters.GetValueOrDefault(type) ?? foundConverters!.GetValueOrDefault(type)
				?? BuiltInConverters.All.GetValueOrDefault(type);
			converters[type] = converter;
			return converter;
		}

		/// <summary>The format a save names, or null when the game has none by that name.</summary>
		public static SaveFormat? Format(string name)
		{
			if (name == SaveFormat.Json.Name || name == SaveFormat.Binary.Name)
			{
				return name == SaveFormat.Json.Name ? SaveFormat.Json : SaveFormat.Binary;
			}

			FindInScripts();
			return addedFormats.GetValueOrDefault(name) ?? foundFormats!.GetValueOrDefault(name);
		}

		/// <summary>The version [SaveVersion] gives a type, or 1, and whether it has one to write.</summary>
		public static (int Version, bool Marked) Version(Type type)
		{
			if (!versions.TryGetValue(type, out var known))
			{
				var marked = type.GetCustomAttribute<SaveVersionAttribute>();
				versions[type] = known = (marked?.Version ?? 1, marked != null);
			}
			return known;
		}

		/// <summary>Each version's migration to the next, from [MigrateFrom] methods and ones added in code.</summary>
		public static SortedDictionary<int, Action<SaveNode>> Migrations(Type type)
		{
			if (migrations.TryGetValue(type, out var known))
			{
				return known;
			}

			var found = new SortedDictionary<int, Action<SaveNode>>();
			foreach (var method in type.GetMethods(Statics))
			{
				var from = method.GetCustomAttribute<MigrateFromAttribute>();
				if (from != null && Migration(type, method) is { } migrate)
				{
					found[from.Version] = migrate;
				}
			}
			foreach (var added in addedMigrations.GetValueOrDefault(type) ?? new SortedDictionary<int, Action<SaveNode>>())
			{
				found[added.Key] = added.Value;
			}
			migrations[type] = found;
			return found;
		}

		private static Action<SaveNode>? Migration(Type type, MethodInfo method)
		{
			var parameters = method.GetParameters();
			if (method.ReturnType == typeof(void) && parameters.Length == 1 && parameters[0].ParameterType == typeof(SaveNode))
			{
				return method.CreateDelegate<Action<SaveNode>>();
			}

			Log.Warning($"{type.Name}.{method.Name} is marked [MigrateFrom], but a migration is a static method that takes a SaveNode and returns nothing, so it never runs.");
			return null;
		}

		public static void Add(SaveConverter converter)
		{
			addedConverters[converter.Type] = converter;
			converters.Clear();
		}

		public static void Add(SaveFormat format)
		{
			addedFormats[format.Name] = format;
		}

		public static void AddMigration(Type type, int from, Action<SaveNode> migrate)
		{
			if (!addedMigrations.TryGetValue(type, out var steps))
			{
				addedMigrations[type] = steps = new SortedDictionary<int, Action<SaveNode>>();
			}
			steps[from] = migrate;
			migrations.Remove(type);
		}

		public static void Search(Assembly assembly)
		{
			if (!searched.Contains(assembly))
			{
				searched.Add(assembly);
				Clear();
			}
		}

		/// <summary>
		/// The name a save writes a type by: its full name, without the assembly, which changes with every build. A
		/// generic type's arguments follow in brackets, and an array's element type comes before [].
		/// </summary>
		public static string NameOf(Type type)
		{
			if (type.IsArray)
			{
				return NameOf(type.GetElementType()!) + "[]";
			}
			if (!type.IsConstructedGenericType)
			{
				return type.FullName ?? type.Name;
			}
			return $"{type.GetGenericTypeDefinition().FullName}[{string.Join(",", type.GetGenericArguments().Select(NameOf))}]";
		}

		/// <summary>The type a save names, looking in the game scripts first; null when the game no longer has it.</summary>
		public static Type? Find(string name)
		{
			if (named.TryGetValue(name, out var known))
			{
				return known;
			}

			var found = Parse(name.Trim());
			named[name] = found;
			return found;
		}

		private static Type? Parse(string name)
		{
			if (name.EndsWith("[]", StringComparison.Ordinal))
			{
				return Find(name[..^2])?.MakeArrayType();
			}

			var open = name.IndexOf('[');
			if (open < 0 || !name.EndsWith(']'))
			{
				return Lookup(name);
			}

			var definition = Lookup(name[..open]);
			var arguments = SplitArguments(name[(open + 1)..^1]).Select(Find).ToArray();
			if (definition == null || Array.Exists(arguments, argument => argument == null))
			{
				return null;
			}
			try
			{
				return definition.MakeGenericType(arguments!);
			}
			catch (ArgumentException)
			{
				return null;
			}
		}

		/// <summary>Splits "A,B[C,D]" at the commas outside brackets.</summary>
		private static List<string> SplitArguments(string inside)
		{
			var parts = new List<string>();
			var depth = 0;
			var start = 0;
			for (var at = 0; at < inside.Length; at++)
			{
				depth += inside[at] == '[' ? 1 : inside[at] == ']' ? -1 : 0;
				if (inside[at] == ',' && depth == 0)
				{
					parts.Add(inside[start..at]);
					start = at + 1;
				}
			}
			parts.Add(inside[start..]);
			return parts;
		}

		private static Type? Lookup(string name)
		{
			foreach (var assembly in Searched())
			{
				var type = assembly.GetType(name);
				if (type != null)
				{
					return type;
				}
			}
			return Type.GetType(name) ?? AppDomain.CurrentDomain.GetAssemblies()
				.Where(assembly => !assembly.IsCollectible)
				.Select(assembly => assembly.GetType(name))
				.FirstOrDefault(type => type != null);
		}

		private static IEnumerable<Assembly> Searched()
		{
			return GameScripts.Assemblies.Concat(searched).Append(typeof(SaveTypes).Assembly);
		}

		/// <summary>Finds the game's converters and formats: classes with a constructor that takes nothing.</summary>
		private static void FindInScripts()
		{
			if (foundConverters != null && foundFormats != null)
			{
				return;
			}

			foundConverters = new Dictionary<Type, SaveConverter>();
			foundFormats = new Dictionary<string, SaveFormat>();
			foreach (var type in GameScripts.Assemblies.Concat(searched).SelectMany(Types))
			{
				if (type.IsAbstract || type.ContainsGenericParameters || type.GetConstructor(Type.EmptyTypes) == null)
				{
					continue;
				}
				if (typeof(SaveConverter).IsAssignableFrom(type) && Make<SaveConverter>(type) is { } converter)
				{
					foundConverters[converter.Type] = converter;
				}
				else if (typeof(SaveFormat).IsAssignableFrom(type) && Make<SaveFormat>(type) is { } format)
				{
					foundFormats[format.Name] = format;
				}
			}
		}

		private static T? Make<T>(Type type) where T : class
		{
			try
			{
				return (T)Activator.CreateInstance(type)!;
			}
			catch (TargetInvocationException exception)
			{
				Log.Warning($"{type.Name} cannot be used for saves, because making one threw: {exception.InnerException?.Message}");
				return null;
			}
		}

		private static IEnumerable<Type> Types(Assembly assembly)
		{
			try
			{
				return assembly.GetTypes();
			}
			catch (ReflectionTypeLoadException exception)
			{
				return exception.Types.Where(type => type != null)!;
			}
		}

		/// <summary>Forgets what was found and worked out, as when the scripts reload.</summary>
		public static void Clear()
		{
			converters.Clear();
			migrations.Clear();
			named.Clear();
			versions.Clear();
			foundConverters = null;
			foundFormats = null;
			SaveMembers.Clear();
			SceneMembers.Clear();
		}

		/// <summary>Drops what the old scripts added in code, so they can leave memory.</summary>
		public static void Forget(AssemblyLoadContext old)
		{
			foreach (var type in addedConverters.Keys.Where(type => GameCode.IsFrom(type, old) || GameCode.Holds(addedConverters[type], old)).ToList())
			{
				addedConverters.Remove(type);
			}
			foreach (var name in addedFormats.Keys.Where(name => GameCode.Holds(addedFormats[name], old)).ToList())
			{
				addedFormats.Remove(name);
			}
			foreach (var type in addedMigrations.Keys.Where(type => GameCode.IsFrom(type, old)).ToList())
			{
				addedMigrations.Remove(type);
			}
			foreach (var steps in addedMigrations.Values)
			{
				foreach (var from in steps.Keys.Where(from => GameCode.Holds(steps[from], old)).ToList())
				{
					steps.Remove(from);
				}
			}
			Save.Forget(old);
			Clear();
		}
	}
}
