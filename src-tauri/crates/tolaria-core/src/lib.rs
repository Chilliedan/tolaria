//! Transport-agnostic core for Tolaria: vault, git, frontmatter, and search
//! logic shared by the desktop (Tauri) app and the web server. No GUI/IPC deps.

pub mod frontmatter;
pub mod process;
pub mod shell_env;
