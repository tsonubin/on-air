//! The HTTP layer sits on top of the domain: `api/` may use anything, but
//! nothing below it may reach back into `crate::api`. `lib.rs` is the crate
//! root that mounts the router, so it is the one allowed exception.

use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_module_outside_api_imports_the_api_layer() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let api = src.join("api");
    let mut files = Vec::new();
    rust_files(&src, &mut files);

    let offenders: Vec<String> = files
        .iter()
        .filter(|path| !path.starts_with(&api) && **path != src.join("lib.rs"))
        .flat_map(|path| {
            let text = std::fs::read_to_string(path).unwrap();
            text.lines()
                .enumerate()
                .filter(|(_, line)| {
                    let code = line.split("//").next().unwrap_or("");
                    code.contains("crate::api") || code.contains("super::api")
                })
                .map(|(number, line)| {
                    format!(
                        "{}:{}: {}",
                        path.strip_prefix(&src).unwrap().display(),
                        number + 1,
                        line.trim()
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "domain modules must not depend on the HTTP layer:\n{}",
        offenders.join("\n")
    );
    assert!(files.len() > 20, "walked the source tree");
}
