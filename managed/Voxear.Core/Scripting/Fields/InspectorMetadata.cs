using System;
using System.Collections.Generic;
using System.Reflection;
using System.Text.Json;

namespace Voxear.Scripting;

/// <summary>
/// What a field's inspector attributes ask of the editor, written beside its kind as an "inspector" object holding
/// only what is set, so the editor shows a tailored Inspector without reading the assembly itself. Condition
/// values are written as a scene writes the field's value, so the editor compares like with like. The drawers
/// that draw the field are written by name, outermost first: "drawers" for its value, or "item_drawers" for each
/// item of a list.
/// </summary>
internal static class InspectorMetadata
{
	public static void Write(Utf8JsonWriter json, FieldInfo field, DrawerTable drawers)
	{
		var attributes = new List<Attribute>();
		foreach (var attribute in field.GetCustomAttributes(inherit: true))
		{
			if (attribute is Attribute known && known.GetType().Namespace == nameof(Voxear))
			{
				attributes.Add(known);
			}
		}
		var item = DrawerTable.ItemType(field.FieldType);
		var chain = item == null ? drawers.Chain(field, field.FieldType) : new List<Type>();
		var items = item == null ? new List<Type>() : drawers.Chain(field, item);
		if (attributes.Count == 0 && chain.Count == 0 && items.Count == 0)
		{
			return;
		}

		json.WriteStartObject("inspector");
		WriteDrawers(json, "drawers", chain);
		WriteDrawers(json, "item_drawers", items);
		var infos = new List<InfoBoxAttribute>();
		foreach (var attribute in attributes)
		{
			if (attribute is InfoBoxAttribute info)
			{
				infos.Add(info);
			}
			else
			{
				WriteOne(json, attribute);
			}
		}
		WriteInfos(json, infos);
		json.WriteEndObject();
	}

	private static void WriteDrawers(Utf8JsonWriter json, string name, List<Type> drawers)
	{
		if (drawers.Count == 0)
		{
			return;
		}

		json.WriteStartArray(name);
		foreach (var drawer in drawers)
		{
			json.WriteStringValue(drawer.FullName);
		}
		json.WriteEndArray();
	}

	/// <summary>A Scene view handle: its kind, and what else it needs, such as its space, axis or colour.</summary>
	private static void WriteHandle(Utf8JsonWriter json, HandleAttribute handle)
	{
		json.WriteStartObject("handle");
		switch (handle)
		{
			case PositionHandleAttribute position:
				json.WriteString("kind", "position");
				json.WriteBoolean("world", position.Space == HandleSpace.World);
				break;
			case RadiusHandleAttribute:
				json.WriteString("kind", "radius");
				break;
			case DirectionHandleAttribute direction:
				json.WriteString("kind", "direction");
				json.WriteBoolean("world", direction.Space == HandleSpace.World);
				break;
			case BoundsHandleAttribute:
				json.WriteString("kind", "bounds");
				break;
			case ArcHandleAttribute arc:
				json.WriteString("kind", "arc");
				json.WriteNumber("radius", arc.Radius);
				break;
			case SliderHandleAttribute slider:
				json.WriteString("kind", "slider");
				json.WriteNumber("axis", (int)slider.Axis);
				break;
		}
		if (handle.Color != null)
		{
			json.WriteString("color", handle.Color);
		}
		json.WriteEndObject();
	}

	private static void WriteOne(Utf8JsonWriter json, Attribute attribute)
	{
		switch (attribute)
		{
			case HandleAttribute handle:
				WriteHandle(json, handle);
				break;
			case GroupAttribute group:
				WriteGroup(json, group);
				break;
			case ConditionAttribute condition:
				WriteCondition(json, condition);
				break;
			case RangeAttribute range:
				WritePair(json, "range", range.Min, range.Max);
				break;
			case ProgressBarAttribute progress:
				WritePair(json, "progress", progress.Min, progress.Max);
				break;
			default:
				WriteValue(json, attribute);
				break;
		}
	}

	/// <summary>The attributes that are one value each, or a flag.</summary>
	private static void WriteValue(Utf8JsonWriter json, Attribute attribute)
	{
		switch (attribute)
		{
			case TitleAttribute title:
				json.WriteString("title", title.Title);
				break;
			case PropertyOrderAttribute order:
				json.WriteNumber("order", order.Order);
				break;
			case IndentAttribute indent:
				json.WriteNumber("indent", indent.Levels);
				break;
			case LabelTextAttribute label:
				json.WriteString("label", label.Text);
				break;
			case MinValueAttribute min:
				json.WriteNumber("min", min.Min);
				break;
			case MaxValueAttribute max:
				json.WriteNumber("max", max.Max);
				break;
			case SuffixAttribute suffix:
				json.WriteString("suffix", suffix.Suffix);
				break;
			case MultilineAttribute multiline:
				json.WriteNumber("multiline", multiline.Lines);
				break;
			case PreviewFieldAttribute preview:
				json.WriteNumber("preview", preview.Size);
				break;
			case RequiredAttribute required:
				json.WriteString("required", required.Message ?? string.Empty);
				break;
			case TooltipAttribute tooltip:
				json.WriteString("tooltip", tooltip.Text);
				break;
			case DocumentationAttribute documentation:
				json.WriteString("documentation", documentation.Page);
				break;
			case ExcerptAttribute excerpt:
				json.WriteString("excerpt", excerpt.Page);
				break;
			case OnValueChangedAttribute changed:
				json.WriteString("on_changed", changed.Method);
				break;
			case ValueDropdownAttribute dropdown:
				json.WriteString("dropdown", dropdown.Member);
				break;
			case ListDrawerSettingsAttribute list:
				json.WriteStartObject("list");
				json.WriteNumber("per_page", Math.Max(list.NumberOfItemsPerPage, 0));
				json.WriteBoolean("index_labels", list.ShowIndexLabels);
				json.WriteEndObject();
				break;
			case ColorPaletteAttribute palette:
				json.WriteStartArray("palette");
				Array.ForEach(palette.Colors, json.WriteStringValue);
				json.WriteEndArray();
				break;
			case ColorUsageAttribute usage when usage.Hdr:
				json.WriteBoolean("hdr", true);
				break;
			case ValidateInputAttribute validate:
				json.WriteStartObject("validate");
				json.WriteString("method", validate.Method);
				json.WriteString("message", validate.Message ?? string.Empty);
				json.WriteEndObject();
				break;
			default:
				WriteFlag(json, attribute);
				break;
		}
	}

	private static void WriteFlag(Utf8JsonWriter json, Attribute attribute)
	{
		var flag = attribute switch
		{
			HideLabelAttribute => "hide_label",
			ReadOnlyAttribute => "read_only",
			HideInInspectorAttribute => "hidden",
			EnumToggleButtonsAttribute => "enum_buttons",
			TableListAttribute => "table",
			_ => null,
		};
		if (flag != null)
		{
			json.WriteBoolean(flag, true);
		}
	}

	private static void WriteGroup(Utf8JsonWriter json, GroupAttribute group)
	{
		var kind = group switch
		{
			FoldoutGroupAttribute => "foldout",
			BoxGroupAttribute => "box",
			HorizontalGroupAttribute => "horizontal",
			TabGroupAttribute => "tab",
			_ => "foldout",
		};
		json.WriteStartObject("group");
		json.WriteString("kind", kind);
		json.WriteString("name", group.Name);
		if (group is TabGroupAttribute tab)
		{
			json.WriteString("tab", tab.Tab);
		}
		json.WriteEndObject();
	}

	private static void WriteCondition(Utf8JsonWriter json, ConditionAttribute condition)
	{
		var name = condition switch
		{
			ShowIfAttribute => "show_if",
			HideIfAttribute => "hide_if",
			EnableIfAttribute => "enable_if",
			_ => "disable_if",
		};
		json.WriteStartObject(name);
		json.WriteString("member", condition.Member);
		json.WritePropertyName("value");
		FieldDefaults.Write(json, condition.Value, condition.Value?.GetType() ?? typeof(object));
		json.WriteEndObject();
	}

	private static void WritePair(Utf8JsonWriter json, string name, double first, double second)
	{
		json.WriteStartArray(name);
		json.WriteNumberValue(first);
		json.WriteNumberValue(second);
		json.WriteEndArray();
	}

	private static void WriteInfos(Utf8JsonWriter json, List<InfoBoxAttribute> infos)
	{
		if (infos.Count == 0)
		{
			return;
		}

		json.WriteStartArray("info");
		foreach (var info in infos)
		{
			json.WriteStartObject();
			json.WriteString("message", info.Message);
			json.WriteString("type", info.Type.ToString().ToLowerInvariant());
			if (info.VisibleIf != null)
			{
				json.WriteString("visible_if", info.VisibleIf);
			}
			json.WriteEndObject();
		}
		json.WriteEndArray();
	}
}
