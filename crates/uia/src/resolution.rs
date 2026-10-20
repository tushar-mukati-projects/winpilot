use std::time::Instant;

use windows::Win32::UI::Accessibility::IUIAutomationElement;
use winpilot_core::{Element, ElementRef, Error, ErrorCode, Result, WindowRef};

use crate::{
    automation::Uia,
    failure::{is_gone, uia_error},
    properties::NodeProps,
    registry::{Known, Missing},
    resolved::Resolved,
};

impl Uia {
    pub fn element(&mut self, id: ElementRef) -> Result<Element> {
        self.resolve(id).map(|resolved| resolved.info)
    }

    pub fn window_of(&mut self, id: ElementRef) -> Result<WindowRef> {
        let known = self.registry.resolve(id, Instant::now()).map_err(|missing| missing_ref(id, missing))?;
        if !known.fingerprint.window.is_known() {
            return Err(Error::not_found(format!("{id} has no owning window")));
        }
        Ok(known.fingerprint.window.hwnd)
    }

    pub(crate) fn resolve(&mut self, id: ElementRef) -> Result<Resolved> {
        let known = self.registry.resolve(id, Instant::now()).map_err(|missing| missing_ref(id, missing))?;
        match self.refresh(&known.live) {
            Ok(props) if known.fingerprint.keeps_identity(props.role, &props.automation_id) => {
                Ok(self.refreshed(id, known, props))
            }
            Ok(_) => {
                self.registry.release_runtime_id(id);
                self.refind(id, &known.fingerprint)
            }
            Err(error) if is_gone(&error) => self.refind(id, &known.fingerprint),
            Err(error) => Err(uia_error(&format!("reading {id}"), &error)),
        }
    }

    fn refreshed(&mut self, id: ElementRef, known: Known<IUIAutomationElement>, props: NodeProps) -> Resolved {
        let Known { live, fingerprint } = known;
        let depth = fingerprint.depth();
        self.registry.update_fingerprint(id, props.fingerprint(fingerprint.window, fingerprint.path));
        Resolved { info: props.into_element(id, depth), element: live }
    }

    fn refresh(&self, live: &IUIAutomationElement) -> windows::core::Result<NodeProps> {
        let updated = unsafe { live.BuildUpdatedCache(&self.requests.element_props) }?;
        Ok(NodeProps::read(&updated))
    }
}

pub(crate) fn missing_ref(id: ElementRef, missing: Missing) -> Error {
    match missing {
        Missing::NeverIssued => {
            Error::new(ErrorCode::StaleRef, format!("{id} is not known (the worker may have restarted); observe again"))
        }
        Missing::FromPreviousWorker => {
            Error::new(ErrorCode::StaleRef, format!("{id} came from a previous worker (it restarted); observe again"))
        }
        Missing::Gone => stale_ref(id),
    }
}

pub(crate) fn stale_ref(id: ElementRef) -> Error {
    Error::new(ErrorCode::StaleRef, format!("{id} no longer exists; observe again"))
}

#[cfg(test)]
mod tests {
    use winpilot_core::Snapshot;

    use super::*;
    use crate::{
        SnapshotOptions, UiaHandle, UiaOptions,
        test_support::{first_element, handle, taskbar},
    };

    #[test]
    fn missing_refs_explain_themselves() {
        let stale = missing_ref(ElementRef(12), Missing::Gone);
        assert_eq!((stale.code, stale.message.as_str()), (ErrorCode::StaleRef, "e12 no longer exists; observe again"));
        let previous = missing_ref(ElementRef(12), Missing::FromPreviousWorker);
        assert_eq!(previous.code, ErrorCode::StaleRef);
        assert_eq!(previous.message, "e12 came from a previous worker (it restarted); observe again");
    }

    #[test]
    fn unknown_refs_are_stale() {
        let error = handle().run(|uia| uia.element(ElementRef(999_999))).unwrap_err();
        assert_eq!(error.code, ErrorCode::StaleRef);
        assert_eq!(error.message, "e999999 is not known (the worker may have restarted); observe again");
        let window = handle().run(|uia| uia.window_of(ElementRef(5))).unwrap_err();
        assert_eq!(window.code, ErrorCode::StaleRef);
    }

    #[test]
    fn refs_from_before_first_ref_belong_to_a_previous_worker() {
        let options = UiaOptions { first_ref: 1_000_001, ..UiaOptions::default() };
        let window = taskbar();
        let (old, fresh) = UiaHandle::spawn(options)
            .unwrap()
            .run(move |uia| {
                let snapshot = uia.snapshot(&window, &SnapshotOptions::default())?;
                Ok((uia.element(ElementRef(12)).unwrap_err(), first_element(&snapshot)?.id))
            })
            .unwrap();
        assert_eq!(old.message, "e12 came from a previous worker (it restarted); observe again");
        assert_eq!(fresh, ElementRef(1_000_001));
    }

    #[test]
    fn element_round_trips_the_root() {
        let window = taskbar();
        let (root, again) = handle()
            .run(move |uia| {
                let snapshot = uia.snapshot(&window, &SnapshotOptions::default())?;
                let root = first_element(&snapshot)?.clone();
                let again = uia.element(root.id)?;
                Ok((root, again))
            })
            .unwrap();
        assert_eq!(again.id, root.id);
        assert_eq!(again.role, root.role);
        assert_eq!(again.class_name, root.class_name);
        assert_eq!(again.automation_id, root.automation_id);
        assert_eq!(again.depth, 0);
    }

    #[test]
    fn window_of_returns_the_snapshot_window() {
        let window = taskbar();
        let hwnd = window.hwnd;
        let found = handle()
            .run(move |uia| {
                let snapshot: Snapshot = uia.snapshot(&window, &SnapshotOptions::default())?;
                let last = snapshot.elements.last().map(|element| element.id);
                let id = last.map_or_else(|| first_element(&snapshot).map(|element| element.id), Ok)?;
                uia.window_of(id)
            })
            .unwrap();
        assert_eq!(found, hwnd);
    }
}
