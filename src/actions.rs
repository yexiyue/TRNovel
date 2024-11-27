use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum Actions {
    SelectedFile(PathBuf),
}
