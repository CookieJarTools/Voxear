// What a build script makes, kept where every build folder under the checkout's `target` finds it, so a new
// build folder copies it in place of making it again. Included by the build scripts that take minutes.

/// The folder kept outputs go in: `target/build-cache`, found from `OUT_DIR`. `None` for a build outside a
/// folder named `target`, which then makes everything itself.
#[allow(dead_code)]
fn build_cache_folder() -> Option<std::path::PathBuf> {
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR")?);
    let target = out.ancestors().find(|folder| folder.file_name().is_some_and(|name| name == "target"))?;
    Some(target.join("build-cache"))
}

/// A 64-bit FNV-1a hash, the same on every run and machine.
#[allow(dead_code)]
struct BuildKey(u64);

#[allow(dead_code)]
impl BuildKey {
    fn new(what: &str) -> Self {
        let mut key = Self(0xcbf2_9ce4_8422_2325);
        key.add(what.as_bytes());
        key
    }

    fn add(&mut self, bytes: &[u8]) {
        for byte in bytes.iter().chain(&(bytes.len() as u64).to_le_bytes()) {
            self.0 = (self.0 ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }

    /// Every file under `folder`, by its path and what it holds, in a set order.
    fn add_folder(&mut self, folder: &std::path::Path) {
        let mut files = Vec::new();
        let mut folders = vec![folder.to_path_buf()];
        while let Some(at) = folders.pop() {
            let Ok(entries) = std::fs::read_dir(&at) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() { folders.push(path) } else { files.push(path) }
            }
        }
        files.sort();
        for file in files {
            self.add(file.to_string_lossy().as_bytes());
            self.add(&std::fs::read(&file).unwrap_or_default());
        }
    }

    /// What a program prints for `--version`, so a new compiler makes everything again.
    fn add_version(&mut self, program: &str) {
        let said = std::process::Command::new(program).arg("--version").output();
        self.add(&said.map(|output| output.stdout).unwrap_or_default());
    }

    fn name(&self, what: &str) -> String {
        format!("{what}-{:016x}", self.0)
    }
}

/// Copies the kept files for `name` into `out`; false when none are kept, or the set is not whole (a folder
/// emptied by a tool that tidies `target`), so the build makes them again.
#[allow(dead_code)]
fn take_kept(name: &str, out: &std::path::Path) -> bool {
    let Some(kept) = build_cache_folder().map(|folder| folder.join(name)) else { return false };
    let Ok(list) = std::fs::read_to_string(kept.join(KEPT_LIST)) else { return false };
    let files: Vec<&str> = list.lines().filter(|line| !line.is_empty()).collect();
    if files.is_empty() || !files.iter().all(|file| std::fs::copy(kept.join(file), out.join(file)).is_ok()) {
        return false;
    }
    // Marked used, so tidying keeps it over sets not used lately.
    let _ = std::fs::File::open(&kept).and_then(|folder| folder.set_modified(std::time::SystemTime::now()));
    true
}

/// Whether a kept set names its files and holds every one.
#[allow(dead_code)]
fn is_whole(kept: &std::path::Path) -> bool {
    std::fs::read_to_string(kept.join(KEPT_LIST))
        .is_ok_and(|list| list.lines().filter(|line| !line.is_empty()).all(|file| kept.join(file).is_file()))
}

/// The file in a kept set naming the files it holds, written last.
#[allow(dead_code)]
const KEPT_LIST: &str = "kept.txt";

/// Keeps `files`, made in `out`, under `name`, and lets go of the sets used longest ago past the `kept` most
/// recent of the same kind. A failure to keep them only means the next build folder makes them again.
#[allow(dead_code)]
fn keep(name: &str, out: &std::path::Path, files: &[std::path::PathBuf], kept: usize) {
    let Some(folder) = build_cache_folder() else { return };
    let partial = folder.join(format!("{name}.{}", std::process::id()));
    let mut list = String::new();
    let made = std::fs::create_dir_all(&partial).is_ok()
        && files.iter().all(|file| {
            file.file_name().is_some_and(|base| {
                list.push_str(&base.to_string_lossy());
                list.push('\n');
                std::fs::copy(out.join(base), partial.join(base)).is_ok()
            })
        })
        && std::fs::write(partial.join(KEPT_LIST), &list).is_ok();
    // Put in place whole, so another build never finds half of it; another may have kept the same set first.
    let whole = folder.join(name);
    if made && whole.is_dir() && !is_whole(&whole) {
        // Left part of by a tool that tidies `target`, or kept before sets named their files.
        let _ = std::fs::remove_dir_all(&whole);
    }
    if !made || std::fs::rename(&partial, &whole).is_err() {
        let _ = std::fs::remove_dir_all(&partial);
    }
    let kind = name.rsplit_once('-').map_or(name, |(kind, _)| kind);
    let Ok(entries) = std::fs::read_dir(&folder) else { return };
    let mut sets: Vec<(std::time::SystemTime, std::path::PathBuf)> = entries
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .rsplit_once('-')
                .is_some_and(|(k, rest)| k == kind && !rest.contains('.'))
        })
        .filter_map(|entry| Some((entry.metadata().ok()?.modified().ok()?, entry.path())))
        .collect();
    sets.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, path) in sets.into_iter().skip(kept) {
        let _ = std::fs::remove_dir_all(path);
    }
}
