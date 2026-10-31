using System;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.Loader;
using Voxear.Scripting;

namespace Voxear.Tests
{
	/// <summary>
	/// Hot reload's state transfer, between two loads of this assembly: the same classes with different identities,
	/// as a script's old build and its new one are.
	/// </summary>
	internal static class ReloadTests
	{
		private const string Keeper = "Voxear.Tests.Reloading.Keeper";

		[Test]
		private static void EveryKindOfValueCarriesOverIntoTheNewBuildsClasses()
		{
			var (old, fresh) = (Load(), Load());
			var (keeper, friend) = (Make(old, Keeper), Make(old, Keeper));
			Call(keeper, "Fill", friend);
			var (newKeeper, newFriend) = (Make(fresh, Keeper), Make(fresh, Keeper));

			var transfer = new StateTransfer(old, fresh.Assemblies.ToList());
			transfer.Replaced[keeper] = newKeeper;
			transfer.Replaced[friend] = newFriend;
			transfer.Copy(keeper, newKeeper, "Keeper");

			var was = (string)Call(keeper, "Describe")!;
			Check.True((string)Call(newKeeper, "Describe")! == was, $"carried: {Call(newKeeper, "Describe")} for {was}");
			Check.True((bool)Call(newKeeper, "Knows", newFriend)!, "references to behaviours go to their replacements");
			var crate = newKeeper.GetType().GetProperty("Crate")!.GetValue(newKeeper)!;
			Check.True(AssemblyLoadContext.GetLoadContext(crate.GetType().Assembly) == fresh, "a class of its own, from the new build");
			Check.True(!(bool)newKeeper.GetType().GetProperty("HasHandler")!.GetValue(newKeeper)!, "a delegate into the old code is not kept");
			Check.True(transfer.Lost.SequenceEqual(new[] { "Keeper: onHit" }), $"and is named: {string.Join(", ", transfer.Lost)}");
		}

		[Test]
		private static void TheOldBuildLeavesMemoryOnceCarriedOver()
		{
			var old = CarryAndUnload();
			for (var i = 0; i < 10 && old.IsAlive; i++)
			{
				GC.Collect();
				GC.WaitForPendingFinalizers();
			}
			Check.True(!old.IsAlive, "nothing the transfer made holds the old build");
		}

		[Test]
		private static void ListenersAndWorkOfTheOldBuildAreToldApart()
		{
			var old = Load();
			var theirType = old.Assemblies.Single().GetType(typeof(ReloadTests).FullName!)!;
			var theirs = theirType.GetMethod(nameof(Listen), BindingFlags.Static | BindingFlags.NonPublic)!.CreateDelegate<Action<string>>();
			Action<string> ours = Listen;
			var both = ours + theirs;
			Check.True(GameCode.Holds(both, old) && !GameCode.Holds(ours, old), "a handler holds the old build when a listener is theirs");
			var left = GameCode.Without(both, old);
			Check.True(left != null && left.GetInvocationList().Single().Method.DeclaringType == typeof(ReloadTests), "only ours is left");

			var crate = old.Assemblies.Single().GetType("Voxear.Tests.Reloading.Crate")!;
			var crates = Activator.CreateInstance(typeof(List<>).MakeGenericType(crate));
			Check.True(GameCode.Holds(crates, old) && !GameCode.Holds(new List<string>(), old), "a list of their class is theirs too");
		}

		/// <summary>Carries a keeper over and unloads the old build, in a frame of its own so nothing here holds it.</summary>
		[MethodImpl(MethodImplOptions.NoInlining)]
		private static WeakReference CarryAndUnload()
		{
			var (old, fresh) = (Load(), Load());
			var (keeper, friend) = (Make(old, Keeper), Make(old, Keeper));
			Call(keeper, "Fill", friend);
			var newKeeper = Make(fresh, Keeper);
			var transfer = new StateTransfer(old, fresh.Assemblies.ToList());
			transfer.Replaced[keeper] = newKeeper;
			transfer.Copy(keeper, newKeeper, "Keeper");
			old.Unload();
			return new WeakReference(old);
		}

		private static void Listen(string text)
		{
		}

		private static GameLoadContext Load()
		{
			var path = typeof(ReloadTests).Assembly.Location;
			var context = new GameLoadContext(path);
			context.LoadFromAssemblyPath(path);
			return context;
		}

		private static Behaviour Make(AssemblyLoadContext context, string type)
		{
			return (Behaviour)Activator.CreateInstance(context.Assemblies.Single().GetType(type)!)!;
		}

		private static object? Call(object target, string method, params object[] arguments)
		{
			return target.GetType().GetMethod(method)!.Invoke(target, arguments);
		}
	}
}
