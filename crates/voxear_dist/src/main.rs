//! `voxear-dist`: packages this checkout as an engine release, laid out as
//! the launcher installs one ("Engine archives" in its design): the programs
//! built in release, `Voxear.Core` and the tool beside it, the settings game
//! scripts build with, the documentation, and `engine.toml` saying what it
//! is. It writes an install folder and a `.tar.zst` of it with every entry at
//! the root. `voxear-release engine add` then signs the archive and lists it.
//!
//! ```bash
//! cargo run -p voxear_dist -- --channel nightly
//! ```
//!
//! The folder is an install in its own right, which `voxup locate` can point
//! the launcher at.

use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail, ensure};
use clap::{Parser, ValueEnum};

/// How hard the archive is compressed: high, since it is made once and
/// downloaded many times.
const COMPRESSION: i32 = 19;

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const PLATFORM: &str = "linux-x86_64";
#[cfg(all(windows, target_arch = "x86_64"))]
const PLATFORM: &str = "windows-x86_64";

/// The programs an install carries in `bin/`.
const PROGRAMS: [&str; 2] = ["vox", "voxear-editor"];

/// The managed assemblies an install carries in `managed/`, each with its
/// symbols, documentation and runtime files where it has them.
const ASSEMBLIES: [&str; 2] = ["Voxear.Core", "Voxear.ScriptInfo"];

#[derive(Parser)]
#[command(name = "voxear-dist", about = "Package this checkout as a Voxear engine release")]
struct Args {
    /// The release channel. A nightly is named for today's date.
    #[arg(long, value_enum, default_value_t = Channel::Nightly)]
    channel: Channel,
    /// The version to name it, instead of the workspace's with the channel's
    /// tag. A beta needs one, such as `0.2.0-beta.1`.
    #[arg(long)]
    version: Option<String>,
    /// Where the install folder and the archive go.
    #[arg(long, default_value = "target/dist")]
    out: PathBuf,
}

#[derive(Clone, Copy, PartialEq, ValueEnum)]
enum Channel {
    Stable,
    Beta,
    Nightly,
}

impl Channel {
    fn name(self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Beta => "beta",
            Channel::Nightly => "nightly",
        }
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize()?;
    let version = version(&args)?;
    let commit = commit(&root)?;
    let name = format!("voxear-{version}-{PLATFORM}");
    println!("Packaging Voxear {version} ({}) for {PLATFORM} from {commit}", args.channel.name());

    build_programs(&root, &version)?;
    build_managed(&root)?;
    let out = root.join(&args.out);
    let install = out.join(&name);
    stage(&root, &install, &engine_file(&version, args.channel, &commit))?;
    let archive = out.join(format!("{name}.tar.zst"));
    pack(&install, &archive)?;

    let size = fs::metadata(&archive)?.len() as f64 / 1_048_576.0;
    println!("Install folder: {}", install.display());
    println!("Archive: {} ({size:.1} MB)", archive.display());
    Ok(())
}

/// The version asked for, or the workspace's with the channel's tag, checked
/// against the channel's rules: no tag for stable, any tag but `nightly` for
/// beta, and `-nightly.YYYYMMDD` for nightly.
fn version(args: &Args) -> Result<String> {
    let base = env!("CARGO_PKG_VERSION");
    let version = match (&args.version, args.channel) {
        (Some(version), _) => version.clone(),
        (None, Channel::Stable) => base.to_string(),
        (None, Channel::Nightly) => format!("{base}-nightly.{}", today()),
        (None, Channel::Beta) => bail!("a beta needs --version, such as {base}-beta.1"),
    };
    let tag = version.split_once('-').map(|(_, tag)| tag);
    let fits = match args.channel {
        Channel::Stable => tag.is_none(),
        Channel::Beta => tag.is_some_and(|tag| !tag.starts_with("nightly")),
        Channel::Nightly => tag.is_some_and(|tag| tag.starts_with("nightly.")),
    };
    ensure!(fits, "{version} does not fit the {} channel", args.channel.name());
    Ok(version)
}

/// Today's date in UTC as `YYYYMMDD`, from days since 1970 (Howard Hinnant's
/// civil-from-days).
fn today() -> String {
    let days = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs() / 86_400) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 { month_index + 3 } else { month_index - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}{month:02}{day:02}")
}

/// The commit packaged, warning when the checkout has changes it does not
/// hold, since the build has them but `engine.toml` cannot say so.
fn commit(root: &Path) -> Result<String> {
    let git = |args: &[&str]| -> Result<String> {
        let output = Command::new("git").arg("-C").arg(root).args(args).output().context("running git")?;
        ensure!(output.status.success(), "git {} failed", args.join(" "));
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    };
    if !git(&["status", "--porcelain"])?.is_empty() {
        eprintln!(
            "warning: the checkout has uncommitted changes, which the build includes but engine.toml cannot name"
        );
    }
    git(&["rev-parse", "--short", "HEAD"])
}

/// `vox` and `voxear-editor`, built in release, knowing their version.
fn build_programs(root: &Path, version: &str) -> Result<()> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = Command::new(cargo)
        .current_dir(root)
        .args(["build", "--release", "-p", "voxear_cli"])
        .env("VOXEAR_VERSION", version)
        .status()
        .context("running cargo")?;
    ensure!(status.success(), "building the programs failed");
    Ok(())
}

/// `Voxear.Core` and `Voxear.ScriptInfo`, built in release.
fn build_managed(root: &Path) -> Result<()> {
    for project in ["managed/Voxear.Core/Voxear.Core.csproj", "managed/Voxear.ScriptInfo/Voxear.ScriptInfo.csproj"] {
        let status = Command::new("dotnet")
            .current_dir(root)
            .args(["build", project, "-c", "Release", "--nologo", "-v", "quiet"])
            .env("DOTNET_NOLOGO", "1")
            .status()
            .context("running dotnet")?;
        ensure!(status.success(), "building {project} failed");
    }
    Ok(())
}

/// What the install is, for the launcher, which reads it rather than running
/// anything. `[supports]` lists the launcher contracts its `vox` answers:
/// none yet, since `vox new` and `vox upgrade` are still to come.
fn engine_file(version: &str, channel: Channel, commit: &str) -> String {
    format!(
        "format = 1\nversion = \"{version}\"\nchannel = \"{}\"\nplatform = \"{PLATFORM}\"\ncommit = \"{commit}\"\n\n\
         # The launcher contracts this install's vox answers: none yet.\n[supports]\n",
        channel.name()
    )
}

/// Lays out the install folder afresh.
fn stage(root: &Path, install: &Path, engine: &str) -> Result<()> {
    if install.exists() {
        fs::remove_dir_all(install).with_context(|| format!("clearing {}", install.display()))?;
    }
    for folder in ["bin", "managed", "docs"] {
        fs::create_dir_all(install.join(folder))?;
    }
    fs::write(install.join("engine.toml"), engine)?;
    copy(&root.join("LICENSE.md"), &install.join("LICENSE.md"))?;

    let release = root.join("target/release");
    for program in PROGRAMS {
        let file = format!("{program}{}", std::env::consts::EXE_SUFFIX);
        copy(&release.join(&file), &install.join("bin").join(&file))?;
    }

    let managed = root.join("target/managed/Release");
    for entry in fs::read_dir(&managed).with_context(|| format!("reading {}", managed.display()))? {
        let path = entry?.path();
        if is_shipped_assembly_file(&path) {
            copy(&path, &install.join("managed").join(path.file_name().unwrap_or_default()))?;
        }
    }
    copy(&root.join("managed/Voxear.Scripts.props"), &install.join("managed/Voxear.Scripts.props"))?;

    copy(&root.join("README.md"), &install.join("docs/README.md"))?;
    for entry in fs::read_dir(root.join("docs"))? {
        let path = entry?.path();
        if path.extension().is_some_and(|extension| extension == "md") {
            copy(&path, &install.join("docs").join(path.file_name().unwrap_or_default()))?;
        }
    }
    Ok(())
}

/// One of the shipped assemblies' files: the library, its symbols and
/// documentation, and the JSON that tells .NET how to run it. The tool's
/// native launcher is left out, since the engine runs it through `dotnet`.
fn is_shipped_assembly_file(path: &Path) -> bool {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let shipped = ["dll", "pdb", "xml", "json"].iter().any(|extension| name.ends_with(&format!(".{extension}")));
    shipped && ASSEMBLIES.iter().any(|assembly| name.starts_with(&format!("{assembly}.")))
}

fn copy(from: &Path, to: &Path) -> Result<()> {
    fs::copy(from, to).with_context(|| format!("copying {} to {}", from.display(), to.display()))?;
    Ok(())
}

/// The install folder as a `.tar.zst` with every entry at the root, folders
/// and files only, in name order and without owners or times, so packaging the
/// same files twice gives the same archive.
fn pack(install: &Path, archive: &Path) -> Result<()> {
    let file = File::create(archive).with_context(|| format!("creating {}", archive.display()))?;
    let mut encoder = zstd::Encoder::new(file, COMPRESSION)?;
    encoder.long_distance_matching(true)?;
    let mut builder = tar::Builder::new(encoder);
    builder.mode(tar::HeaderMode::Deterministic);
    add_folder(&mut builder, install, Path::new(""))?;
    builder.into_inner()?.finish()?;
    Ok(())
}

fn add_folder<W: std::io::Write>(builder: &mut tar::Builder<W>, root: &Path, folder: &Path) -> Result<()> {
    let mut entries: Vec<PathBuf> =
        fs::read_dir(root.join(folder))?.map(|entry| Ok(entry?.path())).collect::<Result<_>>()?;
    entries.sort();
    for path in entries {
        let name = folder.join(path.file_name().unwrap_or_default());
        if path.is_dir() {
            builder.append_dir(&name, &path)?;
            add_folder(builder, root, &name)?;
        } else {
            builder.append_path_with_name(&path, &name)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_fit_their_channels() {
        let args =
            |channel, version: Option<&str>| Args { channel, version: version.map(str::to_string), out: "x".into() };
        assert!(version(&args(Channel::Nightly, None)).unwrap().contains("-nightly.20"));
        assert_eq!(version(&args(Channel::Stable, None)).unwrap(), env!("CARGO_PKG_VERSION"));
        assert!(version(&args(Channel::Beta, None)).is_err(), "a beta names its version");
        assert!(version(&args(Channel::Beta, Some("0.2.0-beta.1"))).is_ok());
        assert!(version(&args(Channel::Stable, Some("0.2.0-beta.1"))).is_err());
        assert!(version(&args(Channel::Nightly, Some("0.2.0-beta.1"))).is_err());
        assert_eq!(today().len(), 8);
    }
}
