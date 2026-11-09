using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using System.Text;
using System.Text.Json;

namespace Voxear.Scripting;

/// <summary>
/// Describes the behaviours of a built game assembly for the editor, as JSON: each behaviour a scene can add, the
/// types it derives from and implements, and the fields BehaviourFields can set on it, with the kind of value each
/// takes and the value a new instance starts with. IComponent structs are described the same way, as components, and
/// Definitions, the types data assets hold, as definitions. The editor reads this rather than the source, so what it
/// shows is what the compiler made. [Serializable] types those fields use are described once each, by name, and every
/// [Command] method is listed for the palette.
/// </summary>
internal static class ScriptCatalog
{
	private const BindingFlags Static = BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.DeclaredOnly;

	public static string Describe(Assembly assembly)
	{
		return Describe([assembly]);
	}

	/// <summary>Describes the game's assembly with its packages' as one, as they run together.</summary>
	public static string Describe(IReadOnlyList<Assembly> assemblies)
	{
		var nested = new SortedSet<string>(StringComparer.Ordinal);
		var types = new Dictionary<string, Type>();
		var drawers = DrawerTable.Of(Loadable(assemblies));
		using var stream = new MemoryStream();
		using (var json = new Utf8JsonWriter(stream, new JsonWriterOptions { Indented = true }))
		{
			json.WriteStartObject();
			json.WriteStartArray("behaviours");
			foreach (var behaviour in Behaviours(assemblies))
			{
				WriteBehaviour(json, behaviour, types, drawers);
			}
			json.WriteEndArray();
			json.WriteStartArray("components");
			foreach (var component in Components(assemblies))
			{
				WriteComponent(json, component, types, drawers);
			}
			json.WriteEndArray();
			json.WriteStartArray("definitions");
			foreach (var definition in DefinitionTypes(assemblies))
			{
				WriteDefinition(json, definition, types, drawers);
			}
			json.WriteEndArray();
			WriteSystems(json, assemblies);
			WriteNestedTypes(json, types, nested, drawers);
			WriteCommands(json, assemblies);
			WriteChecks(json, assemblies);
			WriteWindows(json, assemblies);
			WriteSideBar(json, assemblies);
			json.WriteEndObject();
		}
		return Encoding.UTF8.GetString(stream.ToArray());
	}

	/// <summary>Every behaviour a scene can name: not abstract, and not generic.</summary>
	private static List<Type> Behaviours(IReadOnlyList<Assembly> assemblies)
	{
		var found = new List<Type>();
		foreach (var type in Loadable(assemblies))
		{
			if (type != null && typeof(Behaviour).IsAssignableFrom(type) && !type.IsAbstract && !type.ContainsGenericParameters)
			{
				found.Add(type);
			}
		}
		found.Sort((a, b) => string.CompareOrdinal(a.FullName, b.FullName));
		return found;
	}

	/// <summary>Every component a scene can give an Obj: an unmanaged struct marked IComponent.</summary>
	private static List<Type> Components(IReadOnlyList<Assembly> assemblies)
	{
		var found = new List<Type>();
		foreach (var type in Loadable(assemblies))
		{
			if (type != null && SceneComponentTypes.IsComponent(type))
			{
				found.Add(type);
			}
		}
		found.Sort((a, b) => string.CompareOrdinal(a.FullName, b.FullName));
		return found;
	}

	/// <summary>Every Definition a data asset can hold: not abstract, and not generic.</summary>
	private static List<Type> DefinitionTypes(IReadOnlyList<Assembly> assemblies)
	{
		var found = new List<Type>();
		foreach (var type in Loadable(assemblies))
		{
			if (type != null && typeof(Definition).IsAssignableFrom(type) && !type.IsAbstract && !type.ContainsGenericParameters)
			{
				found.Add(type);
			}
		}
		found.Sort((a, b) => string.CompareOrdinal(a.FullName, b.FullName));
		return found;
	}

	/// <summary>
	/// A Definition, as a behaviour is described: the classes it derives from up to Definition, and its fields with the
	/// values a new data asset starts with. Its [CreateAsset] path is its place in the Create menu.
	/// </summary>
	private static void WriteDefinition(Utf8JsonWriter json, Type type, Dictionary<string, Type> types, DrawerTable drawers)
	{
		json.WriteStartObject();
		json.WriteString("name", type.FullName);
		json.WriteStartArray("bases");
		for (var current = type.BaseType; current != null && current != typeof(Definition); current = current.BaseType)
		{
			json.WriteStringValue(current.FullName);
		}
		json.WriteEndArray();
		json.WriteStartArray("interfaces");
		foreach (var contract in type.GetInterfaces())
		{
			json.WriteStringValue(contract.FullName);
		}
		json.WriteEndArray();
		WriteDocumentation(json, type);
		if (type.GetCustomAttribute<CreateAssetAttribute>(inherit: false) is { } menu)
		{
			json.WriteString("menu", menu.Path);
		}
		WriteFields(json, type, FieldDefaults.Create(type), types, drawers);
		WriteButtons(json, type);
		if (drawers.Inspector(type) is { } inspector)
		{
			json.WriteString("inspector", inspector.FullName);
		}
		json.WriteEndObject();
	}

	/// <summary>A component, as a behaviour is described but with no bases: its fields start as the struct's zero value.</summary>
	private static void WriteComponent(Utf8JsonWriter json, Type type, Dictionary<string, Type> types, DrawerTable drawers)
	{
		json.WriteStartObject();
		json.WriteString("name", type.FullName);
		json.WriteStartArray("bases");
		json.WriteEndArray();
		json.WriteStartArray("interfaces");
		foreach (var contract in type.GetInterfaces())
		{
			json.WriteStringValue(contract.FullName);
		}
		json.WriteEndArray();
		WriteDocumentation(json, type);
		WriteMenu(json, type);
		WriteFields(json, type, Activator.CreateInstance(type), types, drawers, ComponentLayout.Of(type));
		json.WriteEndObject();
	}

	/// <summary>Each system the engine will run: its stage and order, as the editor's Entities panel lists them.</summary>
	private static void WriteSystems(Utf8JsonWriter json, IReadOnlyList<Assembly> assemblies)
	{
		var systems = new List<Type>();
		foreach (var type in Loadable(assemblies))
		{
			if (type != null && type.IsSubclassOf(typeof(SystemBase)) && !type.IsAbstract)
			{
				systems.Add(type);
			}
		}
		systems.Sort((a, b) => string.CompareOrdinal(a.FullName, b.FullName));
		json.WriteStartArray("systems");
		foreach (var system in systems)
		{
			json.WriteStartObject();
			json.WriteString("name", system.FullName);
			json.WriteString("stage", (system.GetCustomAttribute<UpdateInAttribute>()?.Stage ?? Stage.Update).ToString());
			json.WriteNumber("order", system.GetCustomAttribute<ExecutionOrderAttribute>()?.Order ?? 0);
			json.WriteEndObject();
		}
		json.WriteEndArray();
	}

	/// <summary>The assemblies' types, leaving out any that cannot load rather than failing for all of them.</summary>
	private static Type?[] Loadable(IReadOnlyList<Assembly> assemblies)
	{
		var found = new List<Type?>();
		foreach (var assembly in assemblies)
		{
			found.AddRange(Loadable(assembly));
		}
		return found.ToArray();
	}

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

	private static void WriteBehaviour(Utf8JsonWriter json, Type type, Dictionary<string, Type> types, DrawerTable drawers)
	{
		json.WriteStartObject();
		json.WriteString("name", type.FullName);
		json.WriteStartArray("bases");
		for (var current = type.BaseType; current != null && current != typeof(Behaviour); current = current.BaseType)
		{
			json.WriteStringValue(current.FullName);
		}
		json.WriteEndArray();
		json.WriteStartArray("interfaces");
		foreach (var contract in type.GetInterfaces())
		{
			json.WriteStringValue(contract.FullName);
		}
		json.WriteEndArray();
		WriteDocumentation(json, type);
		WriteMenu(json, type);
		WriteFields(json, type, FieldDefaults.Create(type), types, drawers);
		WriteButtons(json, type);
		if (drawers.Inspector(type) is { } inspector)
		{
			json.WriteString("inspector", inspector.FullName);
		}
		if (Behaviour.DrawsGizmos(type))
		{
			json.WriteBoolean("gizmos", true);
		}
		if (type.GetCustomAttribute<GizmoIconAttribute>(inherit: true) is { } icon)
		{
			json.WriteString("icon", IconNames.Of(icon.Icon));
		}
		json.WriteEndObject();
	}

	/// <summary>The behaviour's [Button] methods, which take nothing, with what each button says.</summary>
	private static void WriteButtons(Utf8JsonWriter json, Type type)
	{
		const BindingFlags Instance = BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic;
		json.WriteStartArray("buttons");
		foreach (var method in type.GetMethods(Instance))
		{
			var button = method.GetCustomAttribute<ButtonAttribute>(inherit: true);
			if (button == null || method.GetParameters().Length > 0)
			{
				continue;
			}

			json.WriteStartObject();
			json.WriteString("method", method.Name);
			json.WriteString("label", button.Label ?? string.Empty);
			json.WriteEndObject();
		}
		json.WriteEndArray();
	}

	/// <summary>Where a type's [AddComponentMenu] lists it in Add Component, when it has one.</summary>
	private static void WriteMenu(Utf8JsonWriter json, Type type)
	{
		if (type.GetCustomAttribute<AddComponentMenuAttribute>(inherit: false) is { } menu)
		{
			json.WriteString("menu", menu.Path);
		}
	}

	/// <summary>The VoxDocs page a type's [Documentation] names, for the book button on its Inspector section.</summary>
	private static void WriteDocumentation(Utf8JsonWriter json, Type type)
	{
		var documentation = type.GetCustomAttribute<DocumentationAttribute>(inherit: true);
		if (documentation != null)
		{
			json.WriteString("documentation", documentation.Page);
		}
	}

	/// <summary>
	/// The fields scenes can set, each with its kind and the value it starts with in instance, and, for a component
	/// whose layout is known, where it lies in its bytes.
	/// </summary>
	public static void WriteFields(Utf8JsonWriter json, Type type, object? instance, Dictionary<string, Type> types, DrawerTable drawers, Dictionary<FieldInfo, int>? layout = null)
	{
		json.WriteStartArray("fields");
		foreach (var field in BehaviourFields.Settable(type))
		{
			json.WriteStartObject();
			json.WriteString("name", field.Name);
			json.WritePropertyName("type");
			FieldKinds.Write(json, field.FieldType, types);
			json.WritePropertyName("default");
			FieldDefaults.Write(json, instance == null ? null : field.GetValue(instance), field.FieldType);
			InspectorMetadata.Write(json, field, drawers);
			if (layout != null && layout.TryGetValue(field, out var offset))
			{
				WriteLayout(json, field, offset);
			}
			json.WriteEndObject();
		}
		json.WriteEndArray();
	}

	/// <summary>Where a component's field lies: its offset and size in bytes, and whether a whole number is signed.</summary>
	private static void WriteLayout(Utf8JsonWriter json, FieldInfo field, int offset)
	{
		var (size, signed) = ComponentLayout.Value(field.FieldType);
		json.WriteStartObject("layout");
		json.WriteNumber("offset", offset);
		json.WriteNumber("size", size);
		json.WriteBoolean("signed", signed);
		json.WriteEndObject();
	}

	/// <summary>
	/// Each [Serializable] type the fields use, by name. Describing one can find more, so this goes on until every
	/// type found has been written.
	/// </summary>
	private static void WriteNestedTypes(Utf8JsonWriter json, Dictionary<string, Type> types, SortedSet<string> written, DrawerTable drawers)
	{
		json.WriteStartObject("types");
		while (Next(types, written) is { } type)
		{
			written.Add(type.FullName!);
			json.WriteStartObject(type.FullName!);
			WriteFields(json, type, FieldDefaults.Create(type), types, drawers);
			json.WriteEndObject();
		}
		json.WriteEndObject();
	}

	/// <summary>Each static method marked [Command] that takes nothing and returns nothing, as the palette runs it.</summary>
	/// <summary>The game's editor windows, each with the title and icon its [Window] gives it, if any.</summary>
	private static void WriteWindows(Utf8JsonWriter json, IReadOnlyList<Assembly> assemblies)
	{
		json.WriteStartArray("windows");
		foreach (var type in EditorWindows.In(Loadable(assemblies)))
		{
			if (typeof(SideBarItem).IsAssignableFrom(type))
			{
				continue;
			}

			var named = type.GetCustomAttribute<WindowAttribute>();
			json.WriteStartObject();
			json.WriteString("name", type.FullName);
			if (named != null)
			{
				json.WriteString("title", named.Title);
			}
			if (named is { HasIcon: true })
			{
				json.WriteString("icon", IconNames.Of(named.Icon));
			}
			json.WriteEndObject();
		}
		json.WriteEndArray();
	}

	/// <summary>
	/// What the game adds to the editor's side bar: each [SideBarButton] method, which takes nothing and returns
	/// nothing, with its icon, title and menu, and each SideBarItem.
	/// </summary>
	private static void WriteSideBar(Utf8JsonWriter json, IReadOnlyList<Assembly> assemblies)
	{
		json.WriteStartObject("side_bar");
		json.WriteStartArray("buttons");
		foreach (var type in Loadable(assemblies))
		{
			foreach (var method in type?.GetMethods(BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic) ?? [])
			{
				var button = method.GetCustomAttribute<SideBarButtonAttribute>();
				if (button == null || method.GetParameters().Length > 0 || method.ReturnType != typeof(void))
				{
					continue;
				}

				json.WriteStartObject();
				json.WriteString("title", button.Title);
				json.WriteString("icon", IconNames.Of(button.Icon));
				if (button.Menu != null)
				{
					json.WriteString("menu", button.Menu);
				}
				json.WriteString("type", type!.FullName);
				json.WriteString("method", method.Name);
				json.WriteEndObject();
			}
		}
		json.WriteEndArray();
		json.WriteStartArray("items");
		foreach (var type in EditorWindows.In(Loadable(assemblies)))
		{
			if (typeof(SideBarItem).IsAssignableFrom(type))
			{
				json.WriteStringValue(type.FullName);
			}
		}
		json.WriteEndArray();
		json.WriteEndObject();
	}

	private static void WriteCommands(Utf8JsonWriter json, IReadOnlyList<Assembly> assemblies)
	{
		json.WriteStartArray("commands");
		foreach (var type in Loadable(assemblies))
		{
			if (type == null)
			{
				continue;
			}
			foreach (var method in type.GetMethods(Static))
			{
				WriteCommand(json, type, method);
			}
		}
		json.WriteEndArray();
	}

	private static void WriteCommand(Utf8JsonWriter json, Type type, MethodInfo method)
	{
		var command = method.GetCustomAttribute<CommandAttribute>();
		if (command == null || method.GetParameters().Length > 0 || method.ReturnType != typeof(void))
		{
			return;
		}

		json.WriteStartObject();
		json.WriteString("title", command.Title);
		json.WriteString("keys", command.Keys);
		json.WriteString("type", type.FullName);
		json.WriteString("method", method.Name);
		json.WriteEndObject();
	}

	/// <summary>The <c>[DocsCheck]</c> methods, which a VoxDocs page's Action… menu lists for a guide's steps.</summary>
	private static void WriteChecks(Utf8JsonWriter json, IReadOnlyList<Assembly> assemblies)
	{
		json.WriteStartArray("checks");
		foreach (var type in Loadable(assemblies))
		{
			if (type == null)
			{
				continue;
			}
			foreach (var method in type.GetMethods(Static))
			{
				WriteCheck(json, type, method);
			}
		}
		json.WriteEndArray();
	}

	private static void WriteCheck(Utf8JsonWriter json, Type type, MethodInfo method)
	{
		var check = method.GetCustomAttribute<DocsCheckAttribute>();
		var answers = method.ReturnType == typeof(bool) || method.ReturnType == typeof(string);
		if (check == null || method.GetParameters().Length > 0 || !answers)
		{
			return;
		}

		json.WriteStartObject();
		json.WriteString("title", check.Title);
		json.WriteString("type", type.FullName);
		json.WriteString("method", method.Name);
		json.WriteEndObject();
	}

	private static Type? Next(Dictionary<string, Type> types, SortedSet<string> written)
	{
		foreach (var (name, type) in types)
		{
			if (!written.Contains(name))
			{
				return type;
			}
		}
		return null;
	}
}
