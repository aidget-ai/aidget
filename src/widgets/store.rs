use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::common::error::{Error, Result};
use crate::common::utils::atomic_write;
pub use crate::settings::SETTINGS_FILE;
use crate::settings::{self, Settings};
use crate::widgets::model::{
    validate_id, Board, BrokenWidget, DeletedWidgetSummary, WidgetAck, WidgetDocument,
    WidgetDragMode, WidgetFrame, WidgetListing, WidgetMeta, WidgetPermission, WidgetSummary,
    MAX_WIDGETS,
};
use crate::widgets::retention;

pub const PACKAGE_FILE: &str = "package.json";
pub const CONFIG_FILE: &str = "config.json";
pub const HTML_FILE: &str = "index.html";
pub const CSS_FILE: &str = "style.css";
pub const AGENTS_FILE: &str = "agents.md";
pub const BOOTSTRAP_MARKER: &str = ".bootstrap";
const DELETION_FILE: &str = "deletion.json";
const DELETION_PREVIEW_FILE: &str = "deletion-preview.png";

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeletionRecord {
    frame: Option<WidgetFrame>,
}

/// Fields a whole-file update may replace. Unset fields keep their current
/// value, so callers can replace permissions without changing content.
#[derive(Default, Clone, Copy)]
pub struct WidgetEdit<'a> {
    pub html: Option<&'a str>,
    pub css: Option<&'a str>,
    pub title: Option<&'a str>,
    pub network: Option<&'a [String]>,
    /// Omitted updates preserve the current window drag policy.
    pub drag_mode: Option<WidgetDragMode>,
    /// `Some(&[])` clears every declared permission.
    pub permissions: Option<&'a [WidgetPermission]>,
    pub agents: Option<&'a str>,
}

#[derive(Clone)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Store { root: root.into() }
    }

    /// Serialize installation and author writes across GUI and CLI processes.
    pub fn write_lock(&self) -> Result<fs::File> {
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.root.join(".write.lock"))?;
        fs2::FileExt::lock_exclusive(&file)?;
        Ok(file)
    }

    pub fn default_root() -> PathBuf {
        std::env::var_os("AIDGET_HOME")
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|home| home.join(".aidget")))
            .unwrap_or_else(|| PathBuf::from(".aidget"))
    }

    pub fn open_default() -> Self {
        Store::new(Self::default_root())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn widgets_dir(&self) -> PathBuf {
        self.root.join("widgets")
    }

    pub fn trash_dir(&self) -> PathBuf {
        self.root.join("trash")
    }

    pub fn state_dir(&self) -> PathBuf {
        self.root.join("state")
    }

    pub fn screenshots_dir(&self) -> PathBuf {
        self.state_dir().join("screenshots")
    }

    pub fn skills_dir(&self) -> PathBuf {
        self.root.join("skills")
    }

    /// Path to the strict, user-editable retention configuration.
    pub fn settings_path(&self) -> PathBuf {
        self.root.join(SETTINGS_FILE)
    }

    pub fn run_dir(&self) -> PathBuf {
        self.root.join("run")
    }

    pub fn socket_path(&self) -> PathBuf {
        self.run_dir().join("ui.sock")
    }

    pub fn widget_dir(&self, id: &str) -> Result<PathBuf> {
        validate_id(id)?;
        Ok(self.widgets_dir().join(id))
    }

    pub fn ensure_layout(&self) -> Result<()> {
        for dir in [
            self.root.clone(),
            self.widgets_dir(),
            self.trash_dir(),
            self.state_dir(),
            self.screenshots_dir(),
            self.skills_dir(),
            self.run_dir(),
        ] {
            fs::create_dir_all(dir)?;
        }
        settings::ensure(&self.root)?;
        Ok(())
    }

    /// Read and validate the retention settings. A missing defaults file is
    /// created by [`Store::ensure_layout`].
    pub fn load_settings(&self) -> Result<Settings> {
        settings::load(&self.root)
    }

    /// Keep only the newest trashed widget directories allowed by `settings.toml`.
    pub fn prune_trash(&self) -> Result<()> {
        let settings = self.load_settings()?;
        retention::prune_trash(&self.trash_dir(), settings.trash_retention)
    }

    /// Apply both retention limits after reading `settings.toml` once.
    pub fn prune_retained(&self) -> Result<()> {
        let settings = self.load_settings()?;
        retention::prune_screenshots(&self.screenshots_dir(), settings.screenshot_retention)?;
        retention::prune_trash(&self.trash_dir(), settings.trash_retention)
    }

    /// Write screenshot bytes and prune the managed screenshot directory.
    pub fn write_screenshot(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        retention::write_screenshot(&self.root, &self.screenshots_dir(), path, bytes)
    }

    pub fn board_path(&self) -> PathBuf {
        self.root.join("board.json")
    }

    pub fn load_board(&self) -> Result<Board> {
        let path = self.board_path();
        if !path.exists() {
            return Ok(Board::default());
        }
        let bytes = fs::read(path)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn save_board(&self, board: &Board) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(board)?;
        atomic_write(&self.board_path(), &bytes)
    }

    pub fn widget_ids(&self) -> Result<Vec<String>> {
        let dir = self.widgets_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut ids = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            if name.starts_with('.') || validate_id(&name).is_err() {
                continue;
            }
            if entry.path().join(CONFIG_FILE).is_file() {
                ids.push(name);
            }
        }
        ids.sort();
        Ok(ids)
    }

    pub fn load_meta(&self, id: &str) -> Result<WidgetMeta> {
        let path = self.widget_dir(id)?.join(CONFIG_FILE);
        if !path.is_file() {
            return Err(Error::NotFound(id.to_string()));
        }
        let bytes = fs::read(path)?;
        let meta: WidgetMeta = serde_json::from_slice(&bytes)?;
        if meta.id != id {
            return Err(Error::InvalidParams(format!(
                "config.json id `{}` does not match directory `{id}`",
                meta.id
            )));
        }
        meta.config.validate()?;
        self.load_package(id)?;
        Ok(meta)
    }

    /// Read optional npm metadata; malformed files never become local widgets.
    pub fn load_package(&self, id: &str) -> Result<Option<super::package::PackageManifest>> {
        let path = self.widget_dir(id)?.join(PACKAGE_FILE);
        match fs::read(path) {
            Ok(bytes) => {
                let package: super::package::PackageManifest = serde_json::from_slice(&bytes)?;
                package.validate()?;
                Ok(Some(package))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn load_document(&self, id: &str) -> Result<WidgetDocument> {
        let dir = self.widget_dir(id)?;
        let meta = self.load_meta(id)?;
        let html = read_optional(&dir.join(HTML_FILE))?.unwrap_or_default();
        let css = read_optional(&dir.join(CSS_FILE))?.unwrap_or_default();
        let agents = read_optional(&dir.join(AGENTS_FILE))?;
        let board = self.load_board()?;
        Ok(WidgetDocument {
            package: self.load_package(id)?,
            meta,
            html,
            css,
            agents,
            frame: board.widgets.get(id).cloned(),
        })
    }

    pub fn list(&self) -> Result<WidgetListing> {
        let board = self.load_board()?;
        let mut widgets = Vec::new();
        let mut broken = Vec::new();
        for id in self.widget_ids()? {
            match self.load_meta(&id) {
                Ok(meta) => {
                    let frame = board.widgets.get(&id).cloned();
                    widgets.push(WidgetSummary {
                        id: meta.id.clone(),
                        title: meta.title.clone(),
                        revision: meta.revision,
                        network: meta.network.clone(),
                        drag_mode: meta.drag_mode,
                        permissions: meta.permissions.clone(),
                        frame,
                    });
                }
                Err(error) => broken.push(BrokenWidget {
                    id,
                    error: error.to_string(),
                }),
            }
        }
        Ok(WidgetListing {
            board_revision: board.revision,
            widgets,
            broken,
            displays: None,
        })
    }

    /// Surface invalid trash configurations without blocking healthy trash entries.
    pub fn trash_errors(&self) -> Result<Vec<BrokenWidget>> {
        let mut errors = vec![];
        for entry in fs::read_dir(self.trash_dir())? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let result = fs::read(entry.path().join(CONFIG_FILE))
                .map_err(Error::from)
                .and_then(|bytes| {
                    let meta: WidgetMeta = serde_json::from_slice(&bytes)?;
                    meta.config.validate()
                });
            if let Err(error) = result {
                errors.push(BrokenWidget {
                    id: format!("trash/{}", entry.file_name().to_string_lossy()),
                    error: error.to_string(),
                });
            }
        }
        Ok(errors)
    }

    pub fn deleted_widgets(&self) -> Result<Vec<DeletedWidgetSummary>> {
        let dir = self.trash_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut widgets = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let (created, created_at) = entry_created_at(&entry)?;
            let Some(trash_name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            let path = entry.path();
            let config = path.join(CONFIG_FILE);
            if !config.is_file() {
                continue;
            }
            // Invalid entries are reported separately by trash_errors.
            let Ok(meta) = serde_json::from_slice::<WidgetMeta>(&fs::read(config)?) else {
                continue;
            };
            if meta.config.validate().is_err() {
                continue;
            }
            validate_id(&meta.id)?;
            let frame = deletion_record(&path)?
                .and_then(|record| record.frame)
                .unwrap_or_else(|| WidgetFrame::default_for(widgets.len()));
            widgets.push((
                created,
                DeletedWidgetSummary {
                    trash_name,
                    id: meta.id.clone(),
                    title: meta.title.clone(),
                    width: frame.w,
                    height: frame.h,
                    has_preview: path.join(DELETION_PREVIEW_FILE).is_file(),
                    created_at,
                },
            ));
        }
        widgets.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| right.1.trash_name.cmp(&left.1.trash_name))
        });
        Ok(widgets.into_iter().map(|(_, widget)| widget).collect())
    }

    /// Creation timestamp of the widget directory, in Unix milliseconds.
    pub fn widget_created_at_ms(&self, id: &str) -> Result<u64> {
        let metadata = fs::metadata(self.widget_dir(id)?)?;
        Ok(metadata_created_at(&metadata)?.1)
    }

    /// Read the preview captured immediately before the widget was trashed.
    pub fn deleted_preview(&self, trash_name: &str) -> Result<Option<Vec<u8>>> {
        read_optional_bytes(&self.trash_path(trash_name)?.join(DELETION_PREVIEW_FILE))
    }

    /// Permanently remove one trashed widget directory.
    pub fn purge(&self, trash_name: &str) -> Result<()> {
        let path = self.trash_path(trash_name)?;
        if !path.is_dir() {
            return Err(Error::NotFound(trash_name.to_string()));
        }
        let id = fs::read(path.join(CONFIG_FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<WidgetMeta>(&bytes).ok())
            .map(|meta| meta.id.clone());
        fs::remove_dir_all(path)?;
        if let Some(id) = id {
            crate::widgets::storage::clear(self, &id)?;
        }
        Ok(())
    }

    /// Move a trashed widget back into the widget directory and restore its frame.
    pub fn restore(&self, trash_name: &str) -> Result<WidgetMeta> {
        let _lock = self.write_lock()?;
        let source = self.trash_path(trash_name)?;
        if !source.is_dir() {
            return Err(Error::NotFound(trash_name.to_string()));
        }
        let meta: WidgetMeta = serde_json::from_slice(&fs::read(source.join(CONFIG_FILE))?)?;
        validate_id(&meta.id)?;

        let existing = self.widget_ids()?;
        if existing.iter().any(|id| id == &meta.id) {
            return Err(Error::InvalidParams(format!(
                "widget `{}` already exists",
                meta.id
            )));
        }
        if existing.len() >= MAX_WIDGETS {
            return Err(Error::InvalidParams(format!(
                "widget limit reached ({MAX_WIDGETS})"
            )));
        }

        let record = deletion_record(&source)?;
        let target = self.widget_dir(&meta.id)?;
        fs::rename(&source, &target)?;
        if let Some(preview) = read_optional_bytes(&target.join(DELETION_PREVIEW_FILE))? {
            atomic_write(
                &self
                    .state_dir()
                    .join("previews")
                    .join(format!("{}.png", meta.id)),
                &preview,
            )?;
            fs::remove_file(target.join(DELETION_PREVIEW_FILE))?;
        }
        match fs::remove_file(target.join(DELETION_FILE)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }

        let mut board = self.load_board()?;
        board.widgets.insert(
            meta.id.clone(),
            record
                .and_then(|record| record.frame)
                .unwrap_or_else(|| WidgetFrame::default_for(existing.len())),
        );
        board.revision += 1;
        self.save_board(&board)?;
        Ok(meta)
    }

    fn trash_path(&self, trash_name: &str) -> Result<PathBuf> {
        let mut components = Path::new(trash_name).components();
        if !matches!(
            (components.next(), components.next()),
            (Some(Component::Normal(_)), None)
        ) {
            return Err(Error::InvalidParams(
                "trashName must name one trash directory".to_string(),
            ));
        }
        Ok(self.trash_dir().join(trash_name))
    }

    /// Ack for `create`/`move`: includes the frame this call placed.
    pub fn ack(&self, meta: &WidgetMeta) -> Result<WidgetAck> {
        self.ack_inner(meta, true)
    }

    /// Ack for an edit that never touches `board.json`. The frame is omitted
    /// so an agent cannot read a concurrent user drag as its own side effect.
    pub fn content_ack(&self, meta: &WidgetMeta) -> Result<WidgetAck> {
        self.ack_inner(meta, false)
    }

    fn ack_inner(&self, meta: &WidgetMeta, include_frame: bool) -> Result<WidgetAck> {
        let board = self.load_board()?;
        Ok(WidgetAck {
            widget: meta.id.clone(),
            revision: meta.revision,
            title: meta.title.clone(),
            network: meta.network.clone(),
            drag_mode: meta.drag_mode,
            permissions: meta.permissions.clone(),
            updated_at: meta.updated_at.clone(),
            frame: include_frame
                .then(|| board.widgets.get(&meta.id).cloned())
                .flatten(),
            board_revision: board.revision,
        })
    }

    /// `edit` carries the same fields an update can replace, so the two entry
    /// points stay in sync as capabilities are added.
    pub fn create(&self, id: &str, title: &str, edit: WidgetEdit<'_>) -> Result<WidgetMeta> {
        let _lock = self.write_lock()?;
        validate_id(id)?;
        let existing = self.widget_ids()?;
        if existing.iter().any(|existing| existing == id) {
            return Err(Error::InvalidParams(format!(
                "widget `{id}` already exists"
            )));
        }
        if existing.len() >= MAX_WIDGETS {
            return Err(Error::InvalidParams(format!(
                "widget limit reached ({MAX_WIDGETS})"
            )));
        }

        let dir = self.widget_dir(id)?;

        let mut meta = WidgetMeta::new(id, title);
        meta.drag_mode = edit.drag_mode.unwrap_or_default();
        meta.set_permissions(edit.permissions.unwrap_or(&[]));
        meta.network = edit.network.unwrap_or(&[]).to_vec();
        meta.config.validate()?;
        fs::create_dir_all(dir.join("assets"))?;
        atomic_write(&dir.join(HTML_FILE), edit.html.unwrap_or("").as_bytes())?;
        if let Some(css) = edit.css.filter(|css| !css.is_empty()) {
            atomic_write(&dir.join(CSS_FILE), css.as_bytes())?;
        }
        if let Some(agents) = edit.agents {
            atomic_write(&dir.join(AGENTS_FILE), agents.as_bytes())?;
        }
        atomic_write(&dir.join(CONFIG_FILE), &serde_json::to_vec_pretty(&meta)?)?;

        // A recreated id must not inherit state from a previous widget.
        crate::widgets::storage::clear(self, id)?;
        self.ensure_frame(id, existing.len())?;
        Ok(meta)
    }

    pub fn ensure_frame(&self, id: &str, index: usize) -> Result<()> {
        let mut board = self.load_board()?;
        if board.widgets.contains_key(id) {
            return Ok(());
        }
        board
            .widgets
            .insert(id.to_string(), WidgetFrame::default_for(index));
        board.revision += 1;
        self.save_board(&board)
    }

    pub fn update(&self, id: &str, base_revision: u64, edit: WidgetEdit<'_>) -> Result<WidgetMeta> {
        let _lock = self.write_lock()?;
        let dir = self.widget_dir(id)?;
        let mut meta = self.load_meta(id)?;
        if meta.revision != base_revision {
            return Err(Error::RevisionConflict {
                current: Box::new(crate::common::error::ConflictCurrent::Widget(meta)),
            });
        }

        if let Some(title) = edit.title {
            meta.title = title.to_string();
        }
        if let Some(network) = edit.network {
            meta.network = network.to_vec();
        }
        if let Some(drag_mode) = edit.drag_mode {
            meta.drag_mode = drag_mode;
        }
        if let Some(permissions) = edit.permissions {
            meta.set_permissions(permissions);
        }
        meta.config.validate()?;
        if let Some(html) = edit.html {
            atomic_write(&dir.join(HTML_FILE), html.as_bytes())?;
        }
        if let Some(css) = edit.css {
            atomic_write(&dir.join(CSS_FILE), css.as_bytes())?;
        }
        if let Some(agents) = edit.agents {
            atomic_write(&dir.join(AGENTS_FILE), agents.as_bytes())?;
        }
        meta.revision += 1;
        meta.updated_at = chrono::Local::now().to_rfc3339();
        atomic_write(&dir.join(CONFIG_FILE), &serde_json::to_vec_pretty(&meta)?)?;
        Ok(meta)
    }

    pub fn patch_region(
        &self,
        id: &str,
        base_revision: u64,
        region: &str,
        inner_html: &str,
    ) -> Result<WidgetMeta> {
        self.patch_regions(
            id,
            base_revision,
            &[(region.to_string(), inner_html.to_string())],
        )
    }

    pub fn patch_regions(
        &self,
        id: &str,
        base_revision: u64,
        edits: &[(String, String)],
    ) -> Result<WidgetMeta> {
        let _lock = self.write_lock()?;
        let dir = self.widget_dir(id)?;
        let meta = self.load_meta(id)?;
        if meta.revision != base_revision {
            return Err(Error::RevisionConflict {
                current: Box::new(crate::common::error::ConflictCurrent::Widget(meta)),
            });
        }
        let total: usize = edits.iter().map(|(_, inner)| inner.len()).sum();
        if total > crate::widgets::model::MAX_PAYLOAD {
            return Err(Error::InvalidParams(
                "region payload exceeds 256 KB".to_string(),
            ));
        }
        let html = fs::read_to_string(dir.join(HTML_FILE))?;
        let patched = patch_html_regions(&html, edits)?;
        atomic_write(&dir.join(HTML_FILE), patched.as_bytes())?;

        let mut meta = meta;
        meta.revision += 1;
        meta.updated_at = chrono::Local::now().to_rfc3339();
        atomic_write(&dir.join(CONFIG_FILE), &serde_json::to_vec_pretty(&meta)?)?;
        Ok(meta)
    }

    /// Move a widget into the trash and return the directory it landed in, so
    /// the caller can report a path that actually restores it.
    pub fn delete(&self, id: &str) -> Result<std::path::PathBuf> {
        let _lock = self.write_lock()?;
        let dir = self.widget_dir(id)?;
        if !dir.is_dir() {
            return Err(Error::NotFound(id.to_string()));
        }
        self.load_settings()?;
        let mut board = self.load_board()?;
        let frame = board.widgets.get(id).cloned();
        let preview =
            read_optional_bytes(&self.state_dir().join("previews").join(format!("{id}.png")))?;
        let stamp = chrono::Local::now().format("%Y%m%dT%H%M%S");
        let target = self.trash_dir().join(format!("{id}-{stamp}"));
        fs::create_dir_all(self.trash_dir())?;
        if dir.join(DELETION_PREVIEW_FILE).exists() {
            return Err(Error::InvalidParams(format!(
                "widget `{id}` contains the reserved file {DELETION_PREVIEW_FILE}"
            )));
        }
        fs::rename(&dir, &target)?;
        if let Some(preview) = preview {
            atomic_write(&target.join(DELETION_PREVIEW_FILE), &preview)?;
            fs::remove_file(self.state_dir().join("previews").join(format!("{id}.png")))?;
        }
        atomic_write(
            &target.join(DELETION_FILE),
            &serde_json::to_vec_pretty(&DeletionRecord { frame })?,
        )?;

        if board.widgets.remove(id).is_some() {
            board.revision += 1;
            self.save_board(&board)?;
        }
        self.prune_trash()?;
        Ok(target)
    }

    pub fn move_widget(&self, id: &str, base_revision: u64, frame: WidgetFrame) -> Result<Board> {
        let _lock = self.write_lock()?;
        validate_id(id)?;
        self.load_meta(id)?;
        let mut board = self.load_board()?;
        if board.revision != base_revision {
            return Err(Error::RevisionConflict {
                current: Box::new(crate::common::error::ConflictCurrent::Board(board)),
            });
        }
        board.widgets.insert(id.to_string(), frame);
        board.revision += 1;
        self.save_board(&board)?;
        Ok(board)
    }

    /// Set the initial frame only while the target frame still matches the
    /// snapshot used to calculate it. A concurrent user move is preserved.
    pub fn set_initial_frame(
        &self,
        id: &str,
        expected: Option<&WidgetFrame>,
        frame: WidgetFrame,
    ) -> Result<Board> {
        let _lock = self.write_lock()?;
        validate_id(id)?;
        self.load_meta(id)?;
        let mut board = self.load_board()?;
        let unchanged = match (board.widgets.get(id), expected) {
            (Some(current), Some(expected)) => {
                current.display == expected.display
                    && current.x == expected.x
                    && current.y == expected.y
                    && current.w == expected.w
                    && current.h == expected.h
            }
            (None, None) => true,
            _ => false,
        };
        if !unchanged {
            return Ok(board);
        }
        board.widgets.insert(id.to_string(), frame);
        board.revision += 1;
        self.save_board(&board)?;
        Ok(board)
    }

    pub fn reload_board(&self) -> Result<BTreeMap<String, WidgetFrame>> {
        Ok(self.load_board()?.widgets)
    }
}

fn deletion_record(dir: &Path) -> Result<Option<DeletionRecord>> {
    let Some(bytes) = read_optional(&dir.join(DELETION_FILE))? else {
        return Ok(None);
    };
    Ok(Some(serde_json::from_str(&bytes)?))
}

fn entry_created_at(entry: &fs::DirEntry) -> Result<(std::time::SystemTime, u64)> {
    metadata_created_at(&entry.metadata()?)
}

fn metadata_created_at(metadata: &fs::Metadata) -> Result<(std::time::SystemTime, u64)> {
    let created = metadata.created().or_else(|_| metadata.modified())?;
    let millis = created
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    Ok((created, millis))
}

fn read_optional_bytes(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn read_optional(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub fn patch_html_region(html: &str, region: &str, inner_html: &str) -> Result<String> {
    patch_html_regions(html, &[(region.to_string(), inner_html.to_string())])
}

/// Replace the inner HTML of several regions in one pass.
///
/// Every named region must exist exactly once, so a typo fails the whole batch
/// instead of half-applying it.
pub fn patch_html_regions(html: &str, edits: &[(String, String)]) -> Result<String> {
    use lol_html::{element, rewrite_str, RewriteStrSettings};
    use std::cell::RefCell;
    use std::collections::HashMap;

    if edits.is_empty() {
        return Err(Error::InvalidParams("no regions to patch".to_string()));
    }
    if edits.iter().any(|(region, _)| region.is_empty()) {
        return Err(Error::InvalidParams("region must not be empty".to_string()));
    }
    let mut seen = std::collections::HashSet::new();
    for (region, _) in edits {
        if !seen.insert(region.as_str()) {
            return Err(Error::InvalidParams(format!(
                "region `{region}` appears twice in one patch"
            )));
        }
    }

    let hits = std::rc::Rc::new(RefCell::new(HashMap::new()));
    let mut settings = RewriteStrSettings::new();
    for (region, inner) in edits {
        let selector = format!("[data-aidget-region=\"{}\"]", region.replace('"', "\\\""));
        let key = region.clone();
        let inner = inner.clone();
        let hits = std::rc::Rc::clone(&hits);
        settings = settings.append_element_content_handler(element!(selector, move |el| {
            let mut hits = hits.borrow_mut();
            let count = hits.entry(key.clone()).or_insert(0);
            *count += 1;
            if *count == 1 {
                el.set_inner_content(&inner, lol_html::html_content::ContentType::Html);
            }
            Ok(())
        }));
    }
    let patched = rewrite_str(html, settings)
        .map_err(|error| Error::Other(format!("html rewrite failed: {error}")))?;

    let hits = std::rc::Rc::try_unwrap(hits)
        .map(RefCell::into_inner)
        .unwrap_or_default();
    for (region, _) in edits {
        match hits.get(region).copied().unwrap_or(0) {
            0 => return Err(Error::RegionNotFound(region.clone())),
            1 => {}
            _ => return Err(Error::RegionAmbiguous(region.clone())),
        }
    }
    Ok(patched)
}
