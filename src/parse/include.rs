//! `<include>`: files a document reads in where it names them, as the spec
//! says readers "shall immediately start reading" them.

use std::path::{Component, Path, PathBuf};

use super::{deeper_than, sanitize, IncludeProblem, Warning, MAX_DEPTH as MAX_NESTING};

/// How deep includes may nest, a file including a file and so on.
const MAX_DEPTH: usize = 8;

/// The most bytes a document may read in through its includes, all told. A
/// file that includes another many times, which includes another many
/// times, would otherwise grow without bound.
pub const MAX_INCLUDED_BYTES: usize = 256 << 20;

/// `xml` with each of its `<include>`s, but those inside a `<userData>`,
/// which holds a vendor's own elements, replaced by what the file it names
/// holds inside its root, read by `read`, which takes the file's path as the
/// including file names it, joined to that file's folder. An included file's
/// root must be the element the `<include>` stands in, as the spec says.
/// One that can't be read in is left out, with a [`Warning::IncludeDropped`].
pub(super) fn expand(
    xml: &str,
    read: &mut dyn FnMut(&str) -> Result<String, String>,
    warnings: &mut Vec<Warning>,
) -> String {
    let mut budget = MAX_INCLUDED_BYTES;
    expand_in(xml, &[], read, &mut budget, warnings).unwrap_or_else(|| xml.to_string())
}

/// `xml`, included by `chain`, with its includes read in, or `None` if it
/// isn't well-formed XML or nests its elements deeper than the XML parser's
/// stack allows.
fn expand_in(
    xml: &str,
    chain: &[String],
    read: &mut dyn FnMut(&str) -> Result<String, String>,
    budget: &mut usize,
    warnings: &mut Vec<Warning>,
) -> Option<String> {
    if deeper_than(xml, MAX_NESTING) {
        return None;
    }
    let doc = roxmltree::Document::parse(xml).ok()?;
    let includes: Vec<roxmltree::Node> = doc
        .descendants()
        .filter(|n| n.has_tag_name("include") && n.parent_element().is_some())
        .filter(|n| !n.ancestors().skip(1).any(|a| a.has_tag_name("userData")))
        .collect();
    if includes.is_empty() {
        return Some(xml.to_string());
    }
    let folder = chain.last().map_or(Path::new(""), |f| {
        Path::new(f).parent().unwrap_or(Path::new(""))
    });
    let mut out = xml.to_string();
    for node in includes.iter().rev() {
        let parent = node.parent_element().expect("filtered above");
        let named = node.attribute("file").unwrap_or_default();
        let read_in = one(
            named,
            folder,
            parent.tag_name().name(),
            chain,
            read,
            budget,
            warnings,
        );
        let inner = read_in.unwrap_or_else(|problem| {
            warnings.push(Warning::IncludeDropped {
                file: named.to_string(),
                problem,
            });
            String::new()
        });
        out.replace_range(node.range(), &inner);
    }
    Some(out)
}

/// What the file an `<include>` `named`, in `folder`, holds inside its
/// root, which must be a `tag`, with its own includes read in, or why it
/// can't be read in.
fn one(
    named: &str,
    folder: &Path,
    tag: &str,
    chain: &[String],
    read: &mut dyn FnMut(&str) -> Result<String, String>,
    budget: &mut usize,
    warnings: &mut Vec<Warning>,
) -> Result<String, IncludeProblem> {
    if named.is_empty() {
        return Err(IncludeProblem::NoFile);
    }
    if chain.len() >= MAX_DEPTH {
        return Err(IncludeProblem::TooDeep);
    }
    let file = joined(folder, named);
    if chain.contains(&file) {
        return Err(IncludeProblem::Cycle);
    }
    let text = read(&file).map_err(IncludeProblem::Unreadable)?;
    if text.len() > *budget {
        return Err(IncludeProblem::TooLarge);
    }
    *budget -= text.len();
    let mut deeper = chain.to_vec();
    deeper.push(file);
    let text = sanitize(&text).into_owned();
    let text = expand_in(&text, &deeper, read, budget, warnings).ok_or(IncludeProblem::NotXml)?;
    if deeper_than(&text, MAX_NESTING) {
        return Err(IncludeProblem::NotXml);
    }
    contents(&text, tag).ok_or(IncludeProblem::WrongRoot)
}

/// What `xml`'s root holds, between its tags, if its root is a `tag`.
fn contents(xml: &str, tag: &str) -> Option<String> {
    let doc = roxmltree::Document::parse(xml).ok()?;
    let root = doc.root_element();
    if root.tag_name().name() != tag {
        return None;
    }
    let (first, last) = (root.first_child(), root.last_child());
    Some(match (first, last) {
        (Some(first), Some(last)) => xml[first.range().start..last.range().end].to_string(),
        _ => String::new(),
    })
}

/// `name`, as an `<include>` writes it, joined to `folder`, the folder of the
/// file that includes it.
fn joined(folder: &Path, name: &str) -> String {
    folder.join(name).to_string_lossy().into_owned()
}

/// A reader for [`expand`] of the files beside `map`, the file a document
/// came from, or in a folder under it. A path out of that folder, once links
/// are followed, or one that isn't a regular file, or over
/// [`MAX_INCLUDED_BYTES`], is refused, so a map from elsewhere can't read
/// other files.
pub(super) fn beside(map: &Path) -> impl FnMut(&str) -> Result<String, String> {
    let folder: PathBuf = map
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .to_path_buf();
    move |name: &str| {
        let lexical = Path::new(name)
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
        if !lexical {
            return Err("an included file must be in the map's folder".into());
        }
        let path = folder.join(name);
        let real = path.canonicalize().map_err(|e| e.to_string())?;
        let home = folder.canonicalize().map_err(|e| e.to_string())?;
        if !real.starts_with(home) {
            return Err("an included file must be in the map's folder".into());
        }
        let meta = real.metadata().map_err(|e| e.to_string())?;
        if !meta.is_file() {
            return Err("not a file".into());
        }
        if meta.len() > MAX_INCLUDED_BYTES as u64 {
            return Err("over 256 MiB".into());
        }
        std::fs::read_to_string(real).map_err(|e| e.to_string())
    }
}
