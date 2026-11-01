//! `voxear-dist`: packages this checkout as an engine release, laid out as
//! the launcher installs one ("Engine archives" in its design): the programs
//! built in release, `Voxear.Core` and the tool beside it, the settings game
//! scripts build with, the documentation, the manual VoxDocs shows, and
//! `engine.toml` saying what it is. It writes an install folder and a
//! `.tar.zst` of it with every entry at the root, all but the manual, which
//! goes in an archive of its own, `voxear-<version>-manual.tar.zst`, its
//! entries under `manual/`, so the launcher can offer it as an optional part
//! of an install. `voxear-release engine add` then signs the archive and lists it.
//!
//! ```bash
//! cargo run -p voxear_dist -- --channel nightly
//! ```
//!
//! The folder is an install in its own right, which `voxup locate` can point
//! the launcher at.
//!
//! Run on Windows, it packages for Windows, `voxear-<version>-windows-x86_64`,
//! with the C runtime linked into the programs so an install needs nothing
//! else of Microsoft's. The release workflow (`.github/workflows/release.yml`)
//! runs it on both platforms with one version, and can sign and publish the
//! two archives together.

use std::{
    fs::{self, File},
    io::Read,
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

/// What an install is, at its root.
const ENGINE_FILE: &str = "engine.toml";

/// The folder the manual is in, in the checkout and in an install, and
/// packed into an archive of its own.
const MANUAL: &str = "manual";

/// The programs an install carries in `bin/`: the command line, the editor,
/// and the player `vox build` copies into each build.
const PROGRAMS: [&str; 3] = ["vox", "voxear-editor", "voxear-player"];

/// The managed assemblies an install carries in `managed/`, each with its
/// symbols, documentation and runtime files where it has them.
const ASSEMBLIES: [&str; 3] = ["Voxear.Core", "Voxear.ScriptInfo", "Voxear.Analyzers"];

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
    let files = gather(&root, &target_dir(&root), &engine_file(&version, args.channel, &commit))?;
    stage(&install, &files)?;
    // The manual is an archive of its own, for the launcher to offer as an optional part of an install.
    let (manual, engine): (Vec<Packed>, Vec<Packed>) =
        files.into_iter().partition(|file| file.path.starts_with(MANUAL));
    let archive = out.join(format!("{name}.tar.zst"));
    pack(&archive, &engine)?;
    check(&archive, &engine)?;
    let manual_archive = out.join(format!("voxear-{version}-manual.tar.zst"));
    pack(&manual_archive, &manual)?;
    check(&manual_archive, &manual)?;

    println!("Install folder: {}", install.display());
    println!("Archive: {} ({})", archive.display(), size(&archive)?);
    println!("The manual: {} ({})", manual_archive.display(), size(&manual_archive)?);
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

/// Where cargo builds: `CARGO_TARGET_DIR` when it is set, as on a CI
/// runner with a cache, or the checkout's `target`.
fn target_dir(root: &Path) -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), |dir| root.join(PathBuf::from(dir)))
}

/// The programs, built in release, knowing their version.
fn build_programs(root: &Path, version: &str) -> Result<()> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut command = Command::new(cargo);
    command.current_dir(root).args(["build", "--release", "-p", "voxear_cli"]).env("VOXEAR_VERSION", version);
    // The C runtime inside the programs, so an install runs without the Visual C++ redistributable.
    let flags = std::env::var("RUSTFLAGS").unwrap_or_default();
    if cfg!(windows) && !flags.contains("crt-static") {
        command.env("RUSTFLAGS", format!("{flags} -C target-feature=+crt-static").trim());
    }
    let status = command.status().context("running cargo")?;
    ensure!(status.success(), "building the programs failed");
    Ok(())
}

/// `Voxear.Core` and `Voxear.ScriptInfo`, built in release.
fn build_managed(root: &Path) -> Result<()> {
    let projects = [
        "managed/Voxear.Core/Voxear.Core.csproj",
        "managed/Voxear.ScriptInfo/Voxear.ScriptInfo.csproj",
        "managed/Voxear.Analyzers/Voxear.Analyzers.csproj",
    ];
    for project in projects {
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
/// anything. `[supports]` lists the launcher contracts its `vox` answers, each
/// with its format: `vox new` and `vox upgrade`.
fn engine_file(version: &str, channel: Channel, commit: &str) -> String {
    format!(
        "format = 1\nversion = \"{version}\"\nchannel = \"{}\"\nplatform = \"{PLATFORM}\"\ncommit = \"{commit}\"\n\n\
         # The launcher contracts this install's vox answers.\n[supports]\nnew = {}\nupgrade = {}\n",
        channel.name(),
        voxear_engine::new_project::FORMAT,
        voxear_engine::upgrade::FORMAT
    )
}

/// One file of the install: where it goes, what it holds, and whether it is
/// a program to run.
struct Packed {
    path: PathBuf,
    bytes: Vec<u8>,
    program: bool,
}

/// Every file of the install, read into memory once. The install folder and
/// the archive are both written from these bytes, so nothing just written is
/// read back: on an NTFS drive (Linux's ntfs3), small files freshly written
/// sometimes read back as zeros, which is also why a file that reads as
/// nothing but zeros is refused here.
fn gather(root: &Path, target: &Path, engine: &str) -> Result<Vec<Packed>> {
    let mut files = vec![Packed { path: ENGINE_FILE.into(), bytes: engine.as_bytes().to_vec(), program: false }];
    let mut add = |source: &Path, path: PathBuf, program: bool| -> Result<()> {
        let bytes = fs::read(source).with_context(|| format!("reading {}", source.display()))?;
        ensure!(bytes.is_empty() || bytes.iter().any(|&b| b != 0), "{} read as zeros; package again", source.display());
        files.push(Packed { path, bytes, program });
        Ok(())
    };
    add(&root.join("LICENSE.md"), "LICENSE.md".into(), false)?;
    for program in PROGRAMS {
        let file = format!("{program}{}", std::env::consts::EXE_SUFFIX);
        add(&target.join("release").join(&file), Path::new("bin").join(&file), true)?;
    }
    let managed = root.join("target/managed/Release");
    for entry in fs::read_dir(&managed).with_context(|| format!("reading {}", managed.display()))? {
        let source = entry?.path();
        if is_shipped_assembly_file(&source) {
            add(&source, Path::new("managed").join(source.file_name().unwrap_or_default()), false)?;
        }
    }
    add(&root.join("managed/Voxear.Scripts.props"), "managed/Voxear.Scripts.props".into(), false)?;
    // The manual VoxDocs shows after a project's own pages, pictures and all.
    for source in files_in(&root.join(MANUAL))? {
        add(&source, source.strip_prefix(root)?.to_path_buf(), false)?;
    }
    add(&root.join("README.md"), "docs/README.md".into(), false)?;
    for entry in fs::read_dir(root.join("docs"))? {
        let source = entry?.path();
        if source.extension().is_some_and(|extension| extension == "md") {
            add(&source, Path::new("docs").join(source.file_name().unwrap_or_default()), false)?;
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// An archive's size as people read it: in MB, or in KB for a small one.
fn size(archive: &Path) -> Result<String> {
    let bytes = fs::metadata(archive)?.len() as f64;
    Ok(if bytes < 1_048_576.0 { format!("{:.0} KB", bytes / 1024.0) } else { format!("{:.1} MB", bytes / 1_048_576.0) })
}

/// Every file in `folder` and the folders inside it, but hidden ones.
fn files_in(folder: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(folder).with_context(|| format!("reading {}", folder.display()))? {
        let path = entry?.path();
        if path.file_name().is_some_and(|name| name.to_string_lossy().starts_with('.')) {
            continue;
        }
        if path.is_dir() {
            files.extend(files_in(&path)?);
        } else {
            files.push(path);
        }
    }
    Ok(files)
}

/// One of the shipped assemblies' files: the library, its symbols and
/// documentation, and the JSON that tells .NET how to run it. The tool's
/// native launcher is left out, since the engine runs it through `dotnet`.
fn is_shipped_assembly_file(path: &Path) -> bool {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let shipped = ["dll", "pdb", "xml", "json"].iter().any(|extension| name.ends_with(&format!(".{extension}")));
    shipped && ASSEMBLIES.iter().any(|assembly| name.starts_with(&format!("{assembly}.")))
}

/// Lays out the install folder afresh.
fn stage(install: &Path, files: &[Packed]) -> Result<()> {
    if install.exists() {
        fs::remove_dir_all(install).with_context(|| format!("clearing {}", install.display()))?;
    }
    for file in files {
        let path = install.join(&file.path);
        fs::create_dir_all(path.parent().unwrap_or(install))?;
        fs::write(&path, &file.bytes).with_context(|| format!("writing {}", path.display()))?;
        #[cfg(unix)]
        if file.program {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
        }
    }
    Ok(())
}

/// The install as a `.tar.zst` with every entry at the root, folders and
/// files only, in name order and without owners or times, so packaging the
/// same files twice gives the same archive.
fn pack(archive: &Path, files: &[Packed]) -> Result<()> {
    let file = File::create(archive).with_context(|| format!("creating {}", archive.display()))?;
    let mut encoder = zstd::Encoder::new(file, COMPRESSION)?;
    encoder.long_distance_matching(true)?;
    let mut builder = tar::Builder::new(encoder);
    let mut folders: Vec<&Path> = Vec::new();
    for file in files {
        // Each folder before what is in it, the manual's folders inside folders too.
        let mut new: Vec<&Path> =
            file.path.ancestors().skip(1).filter(|folder| !folder.as_os_str().is_empty()).collect();
        new.retain(|folder| !folders.contains(folder));
        for folder in new.into_iter().rev() {
            builder.append_data(&mut header(tar::EntryType::Directory, 0o755, 0), folder, std::io::empty())?;
            folders.push(folder);
        }
        let mode = if file.program { 0o755 } else { 0o644 };
        let mut header = header(tar::EntryType::Regular, mode, file.bytes.len() as u64);
        builder.append_data(&mut header, &file.path, file.bytes.as_slice())?;
    }
    builder.into_inner()?.finish()?;
    Ok(())
}

/// An entry's header with no owner or time, so it depends only on the file.
fn header(kind: tar::EntryType, mode: u32, size: u64) -> tar::Header {
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(kind);
    header.set_mode(mode);
    header.set_size(size);
    header.set_mtime(0);
    header.set_uid(0);
    header.set_gid(0);
    header
}

/// Reads the archive back and compares every file in it with what went in,
/// byte for byte: a bad archive is signed for good once published.
fn check(archive: &Path, files: &[Packed]) -> Result<()> {
    let mut tar = tar::Archive::new(zstd::Decoder::new(File::open(archive)?)?);
    let mut checked = 0;
    for entry in tar.entries()? {
        let mut entry = entry?;
        if entry.header().entry_type().is_dir() {
            continue;
        }
        let path = entry.path()?.into_owned();
        let mut packed = Vec::new();
        entry.read_to_end(&mut packed)?;
        let expected = files.iter().find(|file| file.path == path).map(|file| file.bytes.as_slice());
        ensure!(expected == Some(packed.as_slice()), "{} in the archive is not what was packed", path.display());
        checked += 1;
    }
    ensure!(checked == files.len(), "the archive holds {checked} files, not {}", files.len());
    println!("Checked the archive's {checked} files against what was packed");
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
