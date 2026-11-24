use crate::host::Hosts;
use lampas_bytecode::Module;
use lampas_syntax::{Diagnostic, Program, Sources};
use std::fmt;
use std::path::{Path, PathBuf};

const EXTENSION: &str = "amp";

pub enum ScriptError {
    Read {
        path: PathBuf,
        error: std::io::Error,
    },
    Check(Diagnostic),
}

impl fmt::Display for ScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, error } => {
                write!(
                    formatter,
                    "the lampas script {} reads: {error}",
                    path.display()
                )
            }
            Self::Check(diagnostic) => diagnostic.fmt(formatter),
        }
    }
}

impl fmt::Debug for ScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

impl std::error::Error for ScriptError {}

impl From<Diagnostic> for ScriptError {
    fn from(diagnostic: Diagnostic) -> Self {
        Self::Check(diagnostic)
    }
}

pub fn compile(name: &str, source: &str, hosts: &Hosts) -> Result<Module, Diagnostic> {
    let mut sources = Sources::new();
    sources.add(name, source);
    assemble(sources, hosts)
}

pub fn compile_folder(root: &Path, hosts: &Hosts) -> Result<Module, ScriptError> {
    let mut sources = Sources::new();
    for path in files(root)? {
        let text = std::fs::read_to_string(&path).map_err(|error| ScriptError::Read {
            path: path.clone(),
            error,
        })?;
        let name = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        sources.add(name, text);
    }
    assemble(sources, hosts).map_err(ScriptError::Check)
}

fn assemble(sources: Sources, hosts: &Hosts) -> Result<Module, Diagnostic> {
    let program = Program::parse(sources)?;
    let checked = lampas_semantic::check(&program, &hosts.signatures())?;
    lampas_compiler::compile(&program, &checked)
}

fn files(root: &Path) -> Result<Vec<PathBuf>, ScriptError> {
    let read = |error: std::io::Error| ScriptError::Read {
        path: root.to_path_buf(),
        error,
    };
    let mut files = std::fs::read_dir(root)
        .map_err(read)?
        .map(|entry| entry.map(|entry| entry.path()).map_err(read))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|path| path.is_file() && path.extension().is_some_and(|kind| kind == EXTENSION))
        .collect::<Vec<_>>();
    files.sort();
    Ok(files)
}
