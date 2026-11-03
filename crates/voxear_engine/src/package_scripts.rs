//! The packages' C# as part of the project's scripts.
//!
//! A package carries no C# project of its own, so each build of the scripts
//! writes one for each package with scripts, at
//! `.voxear/Packages/<name>/<Assembly>.csproj`. It builds the package's
//! `Scripts/` into an assembly named for the package (`orbit-tools` is
//! `OrbitTools`), against Voxear.Core and the assemblies of the packages it
//! uses, with the same settings and `#if` symbols as the game's scripts.
//! `.voxear/Packages.props`, which `Voxear.Scripts.props` imports when it is
//! there, has the game's scripts use every package assembly, so a game
//! script can use a package's types while the game's own csproj stays as
//! the developer wrote it.
//!
//! Once the scripts build, the list of the packages' assemblies goes beside
//! the game's ([`PACKAGE_ASSEMBLIES`]), and `Voxear.Core` loads them into the
//! same context as the game's, so their behaviours, editor windows, drawers
//! and commands are found as the game's own are.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
pub use voxear_scripting::PACKAGE_ASSEMBLIES;

use crate::{
    packages::{self, Contents, Package, assembly_name},
    project::Project,
};

/// The file in `.voxear/` that has the game's scripts use the packages'.
pub const PROPS_FILE: &str = "Packages.props";

/// The folder in `.voxear/` that holds a C# project for each package.
const FOLDER: &str = "Packages";

/// The packages' `Scripts/` folders, one a line, for working out whether a
/// script changed without resolving the packages again.
const SOURCES_FILE: &str = "Sources.txt";

/// A package's scripts, as an assembly of their own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageAssembly {
    /// The package's name, as `orbit-tools`.
    pub package: String,
    /// The assembly's name, as `OrbitTools`.
    pub name: String,
    /// The C# project written for it.
    pub csproj: PathBuf,
    /// The package's `Scripts/` folder, which the project builds from.
    pub scripts: PathBuf,
}

impl PackageAssembly {
    /// The assembly's file, as it is built beside the game's.
    pub fn file_name(&self) -> String {
        format!("{}.dll", self.name)
    }
}

/// Syncs the project's packages, fetching any the machine does not have,
/// and writes the C# projects for their scripts. Returns the packages'
/// assemblies, each after the ones it uses.
pub fn write(project: &Project) -> Result<Vec<PackageAssembly>> {
    let packages = packages::installed(project)?;
    write_for(project, &packages)
}

/// [`write`], for packages already resolved, dependencies first.
pub fn write_for(project: &Project, packages: &[Package]) -> Result<Vec<PackageAssembly>> {
    let folder = project.root.join(".voxear").join(FOLDER);
    let by_name: BTreeMap<&str, &Package> = packages.iter().map(|package| (package.name.as_str(), package)).collect();
    let scripted: BTreeSet<&str> = packages
        .iter()
        .filter(|package| Contents::of(&package.root).runs_code())
        .map(|package| package.name.as_str())
        .collect();
    let mut assemblies: Vec<PackageAssembly> = Vec::new();
    for package in packages.iter().filter(|package| scripted.contains(package.name.as_str())) {
        let name = assembly_name(&package.name);
        if let Some(same) = assemblies.iter().find(|known| known.name.eq_ignore_ascii_case(&name)) {
            bail!(
                "{} and {} would both build to {name}.dll: one of them needs another name",
                same.package,
                package.name
            );
        }
        let root = std::path::absolute(&package.root).with_context(|| format!("finding {}'s folder", package.name))?;
        let csproj = folder.join(&package.name).join(format!("{name}.csproj"));
        let uses = references(package, &by_name, &scripted);
        write_if_changed(&csproj, &package_project(&package.name, &name, &root, &uses))?;
        let scripts = root.join(packages::contents::SCRIPTS_FOLDER);
        assemblies.push(PackageAssembly { package: package.name.clone(), name, csproj, scripts });
    }
    write_props(project, &assemblies)?;
    let sources: String = assemblies.iter().map(|assembly| format!("{}\n", assembly.scripts.display())).collect();
    let sources_file = folder.join(SOURCES_FILE);
    if assemblies.is_empty() {
        remove(&sources_file)?;
    } else {
        write_if_changed(&sources_file, &sources)?;
    }
    Ok(assemblies)
}

/// Lists the packages' assemblies beside the game's, in `folder`, once the
/// scripts built there, for `Voxear.Core` to load with the game's. A
/// project without package scripts has no list.
pub fn write_list(folder: &Path, assemblies: &[PackageAssembly]) -> Result<()> {
    let path = folder.join(PACKAGE_ASSEMBLIES);
    if assemblies.is_empty() {
        return remove(&path);
    }
    let list: String = assemblies.iter().map(|assembly| format!("{}\n", assembly.file_name())).collect();
    write_if_changed(&path, &list)
}

/// The packages' `Scripts/` folders as the last build of the scripts found
/// them, for seeing whether a package's script changed since.
pub fn script_folders(project: &Project) -> Vec<PathBuf> {
    let path = project.root.join(".voxear").join(FOLDER).join(SOURCES_FILE);
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    text.lines().filter(|line| !line.trim().is_empty()).map(PathBuf::from).collect()
}

/// The packages whose assemblies a package's scripts build against: the
/// ones it needs that have scripts, and, through any it needs without
/// scripts, the ones those need.
fn references(package: &Package, by_name: &BTreeMap<&str, &Package>, scripted: &BTreeSet<&str>) -> Vec<String> {
    let mut found = Vec::new();
    let mut seen = BTreeSet::new();
    let mut queue: Vec<&str> = package.dependencies.iter().map(String::as_str).collect();
    while let Some(name) = queue.pop() {
        if !seen.insert(name) {
            continue;
        }
        if scripted.contains(name) {
            found.push(name.to_string());
        } else if let Some(needed) = by_name.get(name) {
            queue.extend(needed.dependencies.iter().map(String::as_str));
        }
    }
    found.sort();
    found
}

/// A package's C# project: its `Scripts/` built as `name`, with the
/// engine's settings from `Voxear.Scripts.props`, as the game's are.
fn package_project(package: &str, name: &str, root: &Path, uses: &[String]) -> String {
    let root = msbuild_path(root);
    let scripts = format!("{root}/{}", packages::contents::SCRIPTS_FOLDER);
    let mut references = String::new();
    for used in uses {
        let csproj = format!("$(MSBuildThisFileDirectory)../{used}/{}.csproj", assembly_name(used));
        references.push_str(&format!("\t\t<ProjectReference Include=\"{csproj}\" />\n"));
    }
    let references =
        if references.is_empty() { String::new() } else { format!("\t<ItemGroup>\n{references}\t</ItemGroup>\n") };
    format!(
        r#"<Project Sdk="Microsoft.NET.Sdk">
	<!--
		Written by Voxear on each build of the scripts: the scripts of the package {package}, from its own folder, built
		as the game's are. VoxearProjectFolder is the package's, so its own string tables give it its Strings.
	-->
	<Import Project="../../Engine.props" Condition="'$(VoxearHome)' == '' and Exists('../../Engine.props')" />
	<PropertyGroup>
		<VoxearPackage>{package}</VoxearPackage>
		<VoxearProjectFolder>{root}/</VoxearProjectFolder>
	</PropertyGroup>
	<Import Project="$(VoxearHome)managed/Voxear.Scripts.props" />
	<PropertyGroup>
		<AssemblyName>{name}</AssemblyName>
		<RootNamespace>{name}</RootNamespace>
		<EnableDefaultCompileItems>false</EnableDefaultCompileItems>
		<!-- [SerializeField] fields are set by the engine, which the compiler cannot see. -->
		<NoWarn>$(NoWarn);CS0649</NoWarn>
	</PropertyGroup>
	<ItemGroup>
		<Compile Include="{scripts}/**/*.cs" Exclude="{scripts}/**/bin/**;{scripts}/**/obj/**;{scripts}/**/.*/**" />
	</ItemGroup>
{references}</Project>
"#
    )
}

/// `.voxear/Packages.props`, which has the game's scripts use every
/// package's assembly; none for a project without package scripts.
fn write_props(project: &Project, assemblies: &[PackageAssembly]) -> Result<()> {
    let path = project.root.join(".voxear").join(PROPS_FILE);
    if assemblies.is_empty() {
        return remove(&path);
    }
    let mut references = String::new();
    for assembly in assemblies {
        let csproj = format!("$(MSBuildThisFileDirectory){FOLDER}/{}/{}.csproj", assembly.package, assembly.name);
        references.push_str(&format!("\t\t<ProjectReference Include=\"{csproj}\" />\n"));
    }
    let props = format!(
        "<Project>\n\t<!-- Written by Voxear on each build of the scripts: the packages' assemblies, for the game's \
         scripts to use. -->\n\t<ItemGroup>\n{references}\t</ItemGroup>\n</Project>\n"
    );
    write_if_changed(&path, &props)
}

/// A path as MSBuild reads it in a project file: its special characters
/// escaped, so a folder called `a;b` or `100%` stays one path, and then as
/// XML text.
fn msbuild_path(path: &Path) -> String {
    let mut text = String::new();
    for c in path.to_string_lossy().trim_end_matches(['/', '\\']).chars() {
        match c {
            '%' | ';' | '$' | '@' | '\'' | '*' | '?' => text.push_str(&format!("%{:02X}", c as u32)),
            '&' => text.push_str("&amp;"),
            '<' => text.push_str("&lt;"),
            '>' => text.push_str("&gt;"),
            '"' => text.push_str("&quot;"),
            c => text.push(c),
        }
    }
    text
}

/// Writes a file unless it holds `text` already. MSBuild builds a project
/// again when its project files are newer than its build, so a file
/// written for nothing would rebuild every script on every build.
fn write_if_changed(path: &Path, text: &str) -> Result<()> {
    if std::fs::read_to_string(path).is_ok_and(|known| known == text) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("making {}", parent.display()))?;
    }
    std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

fn remove(path: &Path) -> Result<()> {
    if path.exists() {
        std::fs::remove_file(path).with_context(|| format!("removing {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn special_characters_in_a_path_stay_one_path() {
        assert_eq!(msbuild_path(Path::new("/home/sam/a;b/100%/R&D/")), "/home/sam/a%3Bb/100%25/R&amp;D");
    }
}
