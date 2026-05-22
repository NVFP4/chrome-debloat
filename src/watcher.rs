use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

use notify::event::{DataChange, EventKind, ModifyKind};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

#[derive(Debug)]
pub(crate) struct ManagedPolicyWatcher {
    _watcher: RecommendedWatcher,
    events: Receiver<()>,
}

impl ManagedPolicyWatcher {
    pub(crate) fn new(path: &Path) -> Result<Self, notify::Error> {
        let (sender, events) = mpsc::channel();
        let path = path.to_path_buf();
        let target_path = path.clone();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if event.is_ok_and(|event| is_managed_policy_write_event(&event, &target_path)) {
                    let _ = sender.send(());
                }
            })?;

        watcher.watch(
            path.parent().unwrap_or(path.as_path()),
            RecursiveMode::NonRecursive,
        )?;

        Ok(Self {
            _watcher: watcher,
            events,
        })
    }

    pub(crate) fn has_events(&self) -> bool {
        let mut has_events = false;
        while self.events.try_recv().is_ok() {
            has_events = true;
        }

        has_events
    }
}

fn is_managed_policy_write_event(event: &notify::Event, target_path: &Path) -> bool {
    matches!(
        event.kind,
        EventKind::Create(_)
            | EventKind::Modify(ModifyKind::Name(_))
            | EventKind::Modify(ModifyKind::Data(
                DataChange::Any | DataChange::Size | DataChange::Content | DataChange::Other
            ))
    ) && event_path_matches(&event.paths, target_path)
}

fn event_path_matches(paths: &[PathBuf], target_path: &Path) -> bool {
    paths.iter().any(|path| path == target_path)
}
