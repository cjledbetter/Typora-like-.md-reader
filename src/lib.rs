//! Typora-RustReader core library modules.

pub mod assets;
pub mod cli;
pub mod dialogs;
pub mod io;
pub mod ipc;
pub mod markdown;
pub mod watcher;
pub mod window;

pub use dialogs::{pick_open_file, pick_save_file};
pub use io::{atomic_write_file, read_file_lossy, DocumentContent};
pub use ipc::{
    escape_json_str, format_error_script, format_external_change_script, format_file_loaded_script,
    format_file_saved_script, format_outbound_js, format_window_title, parse_ipc_command_json,
    ErrorPayload, ExternalChangePayload, FileLoadedPayload, FileSavedPayload, InboundCommand,
    InboundIpcCommand, OutboundCommand, OutboundIpcCall,
};
pub use markdown::{
    calculate_doc_stats, compute_stats, extract_document_stats, extract_headings, extract_toc,
    extract_toc_and_stats, marked_slugify, slugify, slugify_marked, DocStats, DocumentStats,
    HeadingItem, TocItem,
};
pub use watcher::{DocumentWatcher, WatcherEvent};
