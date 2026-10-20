use std::{
    sync::{Mutex, PoisonError},
    time::{Duration, Instant},
};

use windows::Win32::{
    System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
    UI::Accessibility::{
        CUIAutomation8, CoalesceEventsOptions_Enabled, ConnectionRecoveryBehaviorOptions_Enabled, IUIAutomation,
        IUIAutomation2, IUIAutomation6, IUIAutomationElement, IUIAutomationTreeWalker,
    },
};
use windows::core::Interface;
use winpilot_core::{Context, Result};
use winpilot_win32::{finite_millis, window_exists};

use crate::{failure::UiaResult, options::UiaOptions, registry::Registry, requests::CacheRequests, warmed::Warmed};

static CREATION: Mutex<()> = Mutex::new(());

pub struct Uia {
    pub(crate) automation: IUIAutomation,
    pub(crate) requests: CacheRequests,
    pub(crate) walker: IUIAutomationTreeWalker,
    pub(crate) registry: Registry<IUIAutomationElement>,
    pub(crate) warmed: Warmed,
}

impl Uia {
    pub(crate) fn create(options: UiaOptions) -> Result<Self> {
        let _one_at_a_time = CREATION.lock().unwrap_or_else(PoisonError::into_inner);
        let automation: IUIAutomation = unsafe { CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER) }
            .context("creating the UI Automation client (CUIAutomation8)")?;
        configure_timeouts(&automation, options)?;
        configure_connection(&automation)?;
        let control_view =
            unsafe { automation.ControlViewCondition() }.context("creating the control view condition")?;
        let requests = CacheRequests::build(&automation, &control_view)?;
        let walker = unsafe { automation.ControlViewWalker() }.context("creating the control view walker")?;
        let registry = Registry::new(options.first_ref);
        Ok(Self { automation, requests, walker, registry, warmed: Warmed::default() })
    }

    pub(crate) fn evict_stale_refs(&mut self) {
        self.registry.evict(Instant::now(), window_exists);
        self.warmed.prune(window_exists);
    }

    pub(crate) fn within<T>(
        &self,
        limit: Duration,
        doing: &str,
        call: impl FnOnce() -> windows::core::Result<T>,
    ) -> Result<T> {
        let tuned: IUIAutomation2 = self.automation.cast().context("reaching IUIAutomation2")?;
        let usual = unsafe { tuned.TransactionTimeout() }.context("reading the UI Automation transaction timeout")?;
        unsafe { tuned.SetTransactionTimeout(finite_millis(limit)) }
            .context("setting the UI Automation transaction timeout")?;
        let result = call();
        unsafe { tuned.SetTransactionTimeout(usual) }.context("restoring the UI Automation transaction timeout")?;
        result.uia(doing)
    }
}

fn configure_timeouts(automation: &IUIAutomation, options: UiaOptions) -> Result<()> {
    let tuned: IUIAutomation2 = automation.cast().context("reaching IUIAutomation2")?;
    unsafe { tuned.SetConnectionTimeout(finite_millis(options.connection_timeout)) }
        .context("setting the UI Automation connection timeout")?;
    unsafe { tuned.SetTransactionTimeout(finite_millis(options.transaction_timeout)) }
        .context("setting the UI Automation transaction timeout")?;
    unsafe { tuned.SetAutoSetFocus(false) }.context("turning off UI Automation auto-focus")
}

fn configure_connection(automation: &IUIAutomation) -> Result<()> {
    let tuned: IUIAutomation6 = automation.cast().context("reaching IUIAutomation6")?;
    unsafe { tuned.SetCoalesceEvents(CoalesceEventsOptions_Enabled) }.context("turning on event coalescing")?;
    unsafe { tuned.SetConnectionRecoveryBehavior(ConnectionRecoveryBehaviorOptions_Enabled) }
        .context("turning on connection recovery")
}
