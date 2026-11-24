use crate::host::Hosts;
use lampas_bytecode::Module;
use lampas_syntax::{Diagnostic, Program, Sources};
use std::path::{Path, PathBuf};

const EXTENSION: &str = "lampas";

pub fn compile(name: &str, source: &str, hosts: &Hosts) -> Result<Module, Diagnostic> {
    let mut sources = Sources::new();
    sources.add(name, source);
    assemble(sources, hosts)
}

pub fn compile_folder(root: &Path, hosts: &Hosts) -> Result<Module, Diagnostic> {
    let mut sources = Sources::new();
    for path in files(root) {
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("the lampas script {} reads: {error}", path.display()));
        let name = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        sources.add(name, text);
    }
    assemble(sources, hosts)
}

fn assemble(sources: Sources, hosts: &Hosts) -> Result<Module, Diagnostic> {
    let program = Program::parse(sources)?;
    let checked = lampas_semantic::check(&program, &hosts.signatures())?;
    lampas_compiler::compile(&program, &checked)
}

fn files(root: &Path) -> Vec<PathBuf> {
    let mut files = std::fs::read_dir(root)
        .unwrap_or_else(|error| {
            panic!(
                "the lampas scripts folder {} reads: {error}",
                root.display()
            )
        })
        .map(|entry| {
            entry
                .unwrap_or_else(|error| {
                    panic!(
                        "the lampas scripts folder {} reads: {error}",
                        root.display(),
                    )
                })
                .path()
        })
        .filter(|path| path.is_file() && path.extension().is_some_and(|kind| kind == EXTENSION))
        .collect::<Vec<_>>();
    files.sort();
    files
}
