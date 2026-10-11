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
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use xodr::opencrg::CrgGrid;
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
            let (jobs, unknown) = stale_scenes(Path::new(SCENES));
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
    let dir = Path::new(input).parent().unwrap_or(Path::new("."));
    let scene = bake(&xodr, |file| {
        let path = crg_path(dir, file)?;
        CrgGrid::from_path(path).map_err(|e| e.to_string())
    })?;
    for note in &scene.notes {
        eprintln!("warning: {input}: {note}");
    }

    let bytes = serde_json::to_vec(&scene.json).expect("scene serializes");
    fs::write(output, &bytes).map_err(|e| format!("writing {output}: {e}"))?;

    let count = |key: &str| scene.json[key].as_array().map_or(0, Vec::len);
    let mesh = &scene.json["mesh"];
    let length = |key: &str| mesh[key].as_array().map_or(0, Vec::len);
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
        length("positions") / 3,
        length("indices") / 3,
        crg_files,
        bytes.len() / 1024,
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

/// The `(source, scene)` jobs that bake again each scene in `folder` older
/// than its `.xodr`, an OpenCRG file the `.xodr` names, or this program, and
/// the scenes whose source it can't find, which it leaves as they are.
fn stale_scenes(folder: &Path) -> (Vec<(String, String)>, Vec<String>) {
    let modified = |path: &Path| fs::metadata(path).and_then(|m| m.modified()).ok();
    let exporter = env::current_exe().ok().and_then(|exe| modified(&exe));
    let sources = read_sources(folder);
    let (mut jobs, mut unknown) = (Vec::new(), Vec::new());
    for name in scene_names(folder).unwrap_or_default() {
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
        let dir = source.parent().unwrap_or(Path::new("."));
        let crg_times = fs::read_to_string(&source)
            .map(|xodr| crg_files(&xodr))
            .unwrap_or_default()
            .into_iter()
            .filter_map(|file| modified(&dir.join(file)));
        let newest = crg_times.fold(source_time, |a, b| a.max(b));
        let scene_time = modified(&scene);
        if scene_time < Some(newest) || scene_time < exporter {
            jobs.push((
                source.to_string_lossy().into_owned(),
                scene.to_string_lossy().into_owned(),
            ));
        }
    }
    (jobs, unknown)
}

/// Where a map in `dir` finds the CRG `file` it names: a regular file in
/// `dir` or under it. A map from elsewhere may not read other files.
fn crg_path(dir: &Path, file: &str) -> Result<PathBuf, String> {
    let inside = Path::new(file)
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    if !inside {
        return Err("a CRG file must be in the map's folder".into());
    }
    let path = dir.join(file);
    match path.is_file() {
        true => Ok(path),
        false => Err("not a file".into()),
    }
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
    use super::{crg_files, crg_path};

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
    fn a_crg_file_must_be_a_file_in_the_map_s_folder() {
        let dir = std::path::Path::new("../tests/data");
        for file in [
            "/etc/passwd",
            "../Cargo.toml",
            "/dev/zero",
            "nested/../../x.crg",
        ] {
            assert!(crg_path(dir, file).is_err(), "{file}");
        }
        assert!(crg_path(dir, "objects.xodr").is_ok());
        assert!(crg_path(dir, "data").is_err(), "not a file");
    }
}
