//! Bake OpenDRIVE maps and export each as the JSON the three.js viewer reads.
//!
//! ```sh
//! cargo run -p xodr-viewer -- tests/data/*.xodr
//! cargo run -p xodr-viewer -- tests/data/testtrack.xodr /tmp/testtrack.json
//! cargo run -p xodr-viewer -- --refresh
//! ```
//!
//! Each map goes to `viewer/web/<map name>.json`, unless there is one map and
//! an output path after it. A map that fails to load is reported and the rest
//! still export. It then lists every scene in `viewer/web/` in
//! `scenes.json`, which the viewer's map picker reads, and records the
//! `.xodr` each came from in `sources.json`. OpenCRG files are read from
//! beside the `.xodr`.
//!
//! `--refresh` bakes again every scene in `viewer/web/` that is older than
//! its `.xodr` or than this program, so a scene always holds what the
//! current viewer shows. It finds a scene's `.xodr` in `sources.json`, or
//! where that file has moved or has no entry, as
//! `tests/data/<map name>.xodr`. It names the scenes it can't find a source
//! for.

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use xodr::opencrg::{CrgGrid, LoadOptions};
use xodr_viewer::bake;

/// Where scenes go without an output path, and the folder the viewer serves.
const SCENES: &str = "viewer/web";

/// The files in [`SCENES`] that aren't scenes: the scene list the map picker
/// reads, and each scene's source `.xodr`.
const LISTS: [&str; 2] = ["scenes.json", "sources.json"];

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let jobs: Vec<(String, String)> = match args.as_slice() {
        [flag] if flag == "--refresh" => {
            let (jobs, unknown) = match stale_scenes(Path::new(SCENES)) {
                Ok(found) => found,
                Err(e) => {
                    eprintln!("reading {SCENES}: {e}. Run from the repository root.");
                    return ExitCode::FAILURE;
                }
            };
            if !unknown.is_empty() {
                eprintln!(
                    "not refreshed, no .xodr found for: {}. Export each once with \
                     `viewer_export <map.xodr>`, and later refreshes will find it.",
                    unknown.join(", ")
                );
            }
            if jobs.is_empty() {
                let which = if unknown.is_empty() {
                    "every"
                } else {
                    "every other"
                };
                eprintln!("{which} scene in {SCENES} is up to date");
                return ExitCode::SUCCESS;
            }
            jobs
        }
        [input, output] if output.ends_with(".json") => vec![(input.clone(), output.clone())],
        inputs if !inputs.is_empty() && !inputs.iter().any(|a| a.ends_with(".json")) => inputs
            .iter()
            .map(|input| {
                let stem = Path::new(input).file_stem().unwrap_or_default();
                let output = format!("{SCENES}/{}.json", stem.to_string_lossy());
                (input.clone(), output)
            })
            .collect(),
        _ => {
            eprintln!("usage: viewer_export <input.xodr> [output.json]");
            eprintln!("       viewer_export <input.xodr>...");
            eprintln!("       viewer_export --refresh");
            return ExitCode::FAILURE;
        }
    };

    let mut failed = 0;
    let mut written: HashMap<&str, &str> = HashMap::new();
    for (input, output) in &jobs {
        if let Some(first) = written.get(output.as_str()) {
            eprintln!("skipping {input}: {first} already wrote {output}");
            failed += 1;
            continue;
        }
        match export(input, output) {
            Ok(()) => {
                written.insert(output, input);
            }
            Err(e) => {
                eprintln!("{input}: {e}");
                failed += 1;
            }
        }
    }

    let scenes = fs::canonicalize(SCENES).ok();
    let in_scenes = |output: &&str| {
        let folder = Path::new(output)
            .parent()
            .filter(|p| !p.as_os_str().is_empty());
        fs::canonicalize(folder.unwrap_or(Path::new("."))).ok() == scenes
    };
    if scenes.is_some() && written.keys().any(in_scenes) {
        let baked = written
            .iter()
            .filter(|(output, _)| in_scenes(output))
            .map(|(output, input)| (*output, *input));
        if let Err(e) = write_sources(Path::new(SCENES), baked) {
            eprintln!("warning: recording sources in {SCENES}: {e}");
        }
        if let Err(e) = write_scene_list(Path::new(SCENES)) {
            eprintln!("warning: listing scenes in {SCENES}: {e}");
        }
    }
    if failed > 0 {
        eprintln!("{failed} of {} maps not exported", jobs.len());
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Bake one map and write its viewer scene to `output`.
fn export(input: &str, output: &str) -> Result<(), String> {
    let xodr = fs::read_to_string(input).map_err(|e| format!("reading {input}: {e}"))?;
    let dir = map_dir(Path::new(input));
    let include = |file: &str| {
        let bytes = read_inside(dir, &dir.join(file)).map_err(|e| e.to_string())?;
        String::from_utf8(bytes).map_err(|e| e.to_string())
    };
    let scene = bake(&xodr, include, |file| load_crg(dir, file))?;
    for note in &scene.notes {
        eprintln!("warning: {input}: {note}");
    }

    let written = write_whole(Path::new(output), |file| Ok(scene.write(file)?))
        .and_then(|()| fs::metadata(output).map(|m| m.len()));
    let bytes = written.map_err(|e: io::Error| format!("writing {output}: {e}"))?;

    let count = |key: &str| scene.json[key].as_array().map_or(0, Vec::len);
    let crg_files = scene.json["crg"]["surfaces"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|s| s["loaded"] == true)
        .map(|s| &s["file"])
        .collect::<HashSet<_>>()
        .len();
    eprintln!(
        "wrote {output}: {} lanes, {} objects, {} signals, {} road marks, {} vertices, {} triangles, {} CRG files ({} KiB)",
        count("lanes"),
        count("objects"),
        count("signals"),
        count("roadMarks"),
        scene.mesh.positions.len() / 3,
        scene.mesh.indices.len() / 3,
        crg_files,
        bytes / 1024,
    );
    Ok(())
}

/// The file names of the scenes in `folder`.
fn scene_names(folder: &Path) -> std::io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(folder)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if name.ends_with(".json") && !LISTS.contains(&name.as_str()) {
            names.push(name);
        }
    }
    names.sort_by_key(|n| n.to_lowercase());
    Ok(names)
}

/// Write `scenes.json` in `folder`: the sorted names of its scenes.
fn write_scene_list(folder: &Path) -> std::io::Result<()> {
    let names = scene_names(folder)?;
    fs::write(
        folder.join("scenes.json"),
        serde_json::to_vec(&names).expect("names serialize"),
    )
}

/// Each scene's source `.xodr` from `sources.json` in `folder`, by scene
/// file name. Empty if there is no such file.
fn read_sources(folder: &Path) -> HashMap<String, String> {
    fs::read(folder.join("sources.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Record in `sources.json` in `folder` the `.xodr` each of `baked`'s
/// `(scene path, source path)` came from, as an absolute path.
fn write_sources<'a>(
    folder: &Path,
    baked: impl Iterator<Item = (&'a str, &'a str)>,
) -> std::io::Result<()> {
    let mut sources = read_sources(folder);
    for (output, input) in baked {
        let name = Path::new(output).file_name().unwrap_or_default();
        let input = fs::canonicalize(input)?;
        sources.insert(
            name.to_string_lossy().into_owned(),
            input.to_string_lossy().into_owned(),
        );
    }
    let sorted: std::collections::BTreeMap<_, _> = sources.into_iter().collect();
    fs::write(
        folder.join("sources.json"),
        serde_json::to_vec_pretty(&sorted).expect("sources serialize"),
    )
}

/// A map to bake, and the scene to write it to.
type Job = (String, String);

/// Write `path` with `write`, through a file beside it that replaces it only
/// once whole, so a failed write leaves no part of a scene.
fn write_whole(
    path: &Path,
    write: impl FnOnce(&mut io::BufWriter<fs::File>) -> io::Result<()>,
) -> io::Result<()> {
    let mut partial = path.as_os_str().to_owned();
    partial.push(".partial");
    let partial = PathBuf::from(partial);
    let result = (|| {
        let mut file = io::BufWriter::new(fs::File::create(&partial)?);
        write(&mut file)?;
        io::Write::flush(&mut file)?;
        fs::rename(&partial, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&partial);
    }
    result
}

/// The `(source, scene)` jobs that bake again each scene in `folder` older
/// than its `.xodr`, an OpenCRG file the `.xodr` names, or this program, and
/// the scenes whose source it can't find, which it leaves as they are, or
/// why `folder` can't be read.
fn stale_scenes(folder: &Path) -> io::Result<(Vec<Job>, Vec<String>)> {
    let modified = |path: &Path| fs::metadata(path).and_then(|m| m.modified()).ok();
    let exporter = env::current_exe().ok().and_then(|exe| modified(&exe));
    let sources = read_sources(folder);
    let (mut jobs, mut unknown) = (Vec::new(), Vec::new());
    for name in scene_names(folder)? {
        let scene = folder.join(&name);
        let stem = name.trim_end_matches(".json");
        let source = sources
            .get(&name)
            .map(PathBuf::from)
            .filter(|path| path.exists())
            .unwrap_or_else(|| Path::new("tests/data").join(format!("{stem}.xodr")));
        let Some(source_time) = modified(&source) else {
            unknown.push(name);
            continue;
        };
        let dir = map_dir(&source);
        let crg_times = fs::read_to_string(&source)
            .map(|xodr| crg_files(&xodr))
            .unwrap_or_default()
            .into_iter()
            .filter_map(|file| inside(dir, &dir.join(file)).ok())
            .filter_map(|path| modified(&path));
        let newest = crg_times.fold(source_time, |a, b| a.max(b));
        let scene_time = modified(&scene);
        if scene_time < Some(newest) || scene_time < exporter {
            jobs.push((
                source.to_string_lossy().into_owned(),
                scene.to_string_lossy().into_owned(),
            ));
        }
    }
    Ok((jobs, unknown))
}

/// The folder of the map at `input`, `.` for a bare file name.
fn map_dir(input: &Path) -> &Path {
    input
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}

/// `path` with links followed, if that is in `dir` or under it.
fn inside(dir: &Path, path: &Path) -> io::Result<PathBuf> {
    let real = path.canonicalize()?;
    match real.starts_with(dir.canonicalize()?) {
        true => Ok(real),
        false => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "a file the map names must be in its folder or below it",
        )),
    }
}

/// The longest CRG file `viewer_export` reads, 1 GiB.
const MAX_CRG_BYTES: u64 = 1 << 30;

/// The CRG `file` a map in `dir` names, with the files it includes, each
/// read by [`read_inside`] so a map from elsewhere can't read other files.
/// An include is found beside `file`.
fn load_crg(dir: &Path, file: &str) -> Result<CrgGrid, String> {
    let path = dir.join(file);
    let bytes = read_inside(dir, &path).map_err(|e| e.to_string())?;
    let base = path.parent().unwrap_or(dir).to_path_buf();
    CrgGrid::from_bytes_with(&bytes, &LoadOptions::default(), |name| {
        read_inside(dir, &base.join(name))
    })
    .map_err(|e| e.to_string())
}

/// The bytes of the file at `path`, if it is a regular file in `dir` or
/// under it once links are followed, and at most [`MAX_CRG_BYTES`] long.
fn read_inside(dir: &Path, path: &Path) -> io::Result<Vec<u8>> {
    let refuse = |why: &str| io::Error::new(io::ErrorKind::PermissionDenied, why);
    let real = inside(dir, path)?;
    let meta = real.metadata()?;
    if !meta.is_file() {
        return Err(refuse("not a file"));
    }
    if meta.len() > MAX_CRG_BYTES {
        return Err(refuse("over 1 GiB"));
    }
    fs::read(real)
}

/// The `file` of each `<CRG>` in `xodr`, found by a scan of its text rather
/// than a full load, so a refresh stays quick.
fn crg_files(xodr: &str) -> Vec<String> {
    xodr.split("<CRG")
        .skip(1)
        .filter_map(|tag| {
            let tag = &tag[..tag.find('>')?];
            let at = tag.find("file=")? + "file=".len();
            let quote = tag[at..]
                .chars()
                .next()
                .filter(|c| matches!(c, '"' | '\''))?;
            let value = &tag[at + 1..];
            let value = &value[..value.find(quote)?];
            Some(
                value
                    .replace("&quot;", "\"")
                    .replace("&apos;", "'")
                    .replace("&lt;", "<")
                    .replace("&gt;", ">")
                    .replace("&amp;", "&"),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{crg_files, load_crg, map_dir, read_inside};

    #[test]
    fn the_crg_files_of_a_map_are_found_by_name() {
        let xodr = r#"<road><surface><CRG file="a.crg" mode="attached"/>
            <CRG mode="genuine" file='b &amp; c.crg'/></surface></road>"#;
        assert_eq!(crg_files(xodr), ["a.crg", "b & c.crg"]);
    }

    #[test]
    fn an_unquoted_or_oddly_quoted_file_is_skipped_not_a_panic() {
        assert!(crg_files("<CRG file=éx.crg/><CRG file=“a.crg”/>").is_empty());
    }

    #[test]
    fn a_bare_map_name_is_in_the_working_folder() {
        let path = std::path::Path::new;
        assert_eq!(map_dir(path("crg.xodr")), path("."));
        assert_eq!(map_dir(path("tests/data/crg.xodr")), path("tests/data"));
        assert!(read_inside(map_dir(path("Cargo.toml")), path("Cargo.toml")).is_ok());
    }

    #[test]
    fn a_crg_file_must_be_a_file_in_the_map_s_folder() {
        let root = std::env::temp_dir().join(format!("xodr-viewer-crg-{}", std::process::id()));
        let map = root.join("map");
        fs::create_dir_all(map.join("nested")).unwrap();
        fs::write(root.join("secret.crg"), "outside").unwrap();
        fs::write(map.join("nested/inside.crg"), "inside").unwrap();
        std::os::unix::fs::symlink(root.join("secret.crg"), map.join("link.crg")).unwrap();
        let read = |file: &str| read_inside(&map, &map.join(file));
        assert_eq!(read("nested/inside.crg").unwrap(), b"inside");
        for file in [
            "../secret.crg",
            "link.crg",
            "/etc/passwd",
            "/dev/zero",
            "nested",
        ] {
            assert!(read(file).is_err(), "{file}");
        }
        let include = "$ROAD_CRG_FILE\n../secret.crg\n$\n";
        fs::write(map.join("include.crg"), include).unwrap();
        let err = load_crg(&map, "include.crg").unwrap_err();
        assert!(err.contains("folder"), "{err}");
        fs::remove_dir_all(root).unwrap();
    }
}
