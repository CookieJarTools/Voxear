using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Reflection.Metadata;
using System.Reflection.Metadata.Ecma335;
using System.Reflection.PortableExecutable;

namespace Voxear.ScriptInfo
{
	/// <summary>
	/// The folder each type of an assembly is written in, from its debug symbols, relative to the folder all its
	/// sources share: <c>Inspector/Layout</c> for a type in <c>managed/Voxear.Core/Inspector/Layout</c>. The Code
	/// Reference nests its pages the same way, so a project's types are found where its people put them. Without
	/// symbols every type is at the top.
	/// </summary>
	internal sealed class SourceFolders : IDisposable
	{
		/// <summary>The compiler's note of the files a type with no code of its own is in, as a delegate or an enum.</summary>
		private static readonly Guid TypeDocuments = new Guid("932E74BC-DBA9-4478-8D46-0F32A7BAB3D3");

		private readonly MetadataReaderProvider? provider;
		private readonly MetadataReader? symbols;
		private readonly string root = string.Empty;

		public SourceFolders(Assembly assembly)
		{
			if (!Open(assembly.Location, out provider))
			{
				return;
			}

			symbols = provider!.GetMetadataReader();
			var files = symbols.Documents.Select(d => Normal(symbols.GetString(symbols.GetDocument(d).Name))).Where(Written).ToList();
			root = SharedFolder(files);
		}

		public void Dispose()
		{
			provider?.Dispose();
		}

		/// <summary>The folder a type is written in, from the root, or nothing for the root or when it is not known.</summary>
		public string Of(Type type)
		{
			if (type.DeclaringType != null)
			{
				return Of(type.DeclaringType);
			}

			var file = FileOfCode(type) ?? FileOfType(type);
			return file == null ? string.Empty : Relative(file);
		}

		private static bool Open(string assembly, out MetadataReaderProvider? provider)
		{
			provider = null;
			if (string.IsNullOrEmpty(assembly) || !File.Exists(assembly))
			{
				return false;
			}

			using var stream = File.OpenRead(assembly);
			using var image = new PEReader(stream);
			try
			{
				return image.TryOpenAssociatedPortablePdb(assembly, path => File.Exists(path) ? File.OpenRead(path) : null, out provider, out _) && provider != null;
			}
			catch (BadImageFormatException)
			{
				return false;
			}
		}

		/// <summary>The file one of a type's methods is written in, from where its code's lines are.</summary>
		private string? FileOfCode(Type type)
		{
			if (symbols == null)
			{
				return null;
			}

			const BindingFlags Declared = BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance | BindingFlags.Static | BindingFlags.DeclaredOnly;
			foreach (var method in type.GetMethods(Declared).Cast<MethodBase>().Concat(type.GetConstructors(Declared)))
			{
				var file = FileOf(method);
				if (file != null && Written(file))
				{
					return file;
				}
			}

			return null;
		}

		private string? FileOfType(Type type)
		{
			if (symbols == null)
			{
				return null;
			}

			var handle = MetadataTokens.TypeDefinitionHandle(type.MetadataToken);
			foreach (var info in symbols.GetCustomDebugInformation(handle))
			{
				var entry = symbols.GetCustomDebugInformation(info);
				if (symbols.GetGuid(entry.Kind) != TypeDocuments)
				{
					continue;
				}

				var blob = symbols.GetBlobReader(entry.Value);
				var document = MetadataTokens.DocumentHandle(blob.ReadCompressedInteger());
				return Normal(symbols.GetString(symbols.GetDocument(document).Name));
			}

			return null;
		}

		private string? FileOf(MethodBase method)
		{
			var handle = MetadataTokens.MethodDefinitionHandle(method.MetadataToken);
			var debug = symbols!.GetMethodDebugInformation(handle);
			var document = debug.Document;
			if (document.IsNil)
			{
				document = debug.GetSequencePoints().Select(p => p.Document).FirstOrDefault(d => !d.IsNil);
			}

			return document.IsNil ? null : Normal(symbols.GetString(symbols.GetDocument(document).Name));
		}

		/// <summary>A file's folder from the root, with forward slashes, or nothing for the root itself.</summary>
		private string Relative(string file)
		{
			var folder = Path.GetDirectoryName(file)?.Replace('\\', '/') ?? string.Empty;
			if (!folder.StartsWith(root, StringComparison.Ordinal))
			{
				return string.Empty;
			}

			return folder.Substring(root.Length).Trim('/');
		}

		/// <summary>The folder every file is in, the longest such.</summary>
		private static string SharedFolder(List<string> files)
		{
			if (files.Count == 0)
			{
				return string.Empty;
			}

			var shared = Path.GetDirectoryName(files[0])?.Replace('\\', '/') ?? string.Empty;
			foreach (var file in files.Skip(1))
			{
				while (shared.Length > 0 && !file.StartsWith(shared + "/", StringComparison.Ordinal))
				{
					var cut = shared.LastIndexOf('/');
					shared = cut < 0 ? string.Empty : shared.Substring(0, cut);
				}
			}

			return shared;
		}

		private static string Normal(string path)
		{
			return path.Replace('\\', '/');
		}

		/// <summary>Whether a file is one a person wrote, not one the build generated under obj.</summary>
		private static bool Written(string file)
		{
			return file.EndsWith(".cs", StringComparison.OrdinalIgnoreCase) && !file.Contains("/obj/", StringComparison.Ordinal);
		}
	}
}
