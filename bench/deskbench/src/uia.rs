use std::ffi::c_void;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize};
use windows::Win32::System::Variant::{VARIANT, VT_BOOL, VT_I4};
use windows::Win32::UI::Accessibility::*;
use windows::core::Interface;

use crate::util::{err_str, ms_since};

#[derive(Clone)]
pub struct Target {
    pub label: String,
    pub hwnd: isize,
    pub process: String,
    pub class: String,
    pub center: (i32, i32),
}

#[derive(Default, Clone, Copy)]
pub struct Summary {
    pub count: usize,
    pub interactive_onscreen: usize,
    pub onscreen: usize,
    pub invokable: usize,
    pub text_chars: usize,
    pub truncated: bool,
}

pub struct VariantStat {
    pub name: &'static str,
    pub runs: Vec<f64>,
    pub summary: Summary,
    pub err: Option<String>,
}

pub struct TargetResult {
    pub target: Target,
    pub cold_ms: f64,
    pub cold_summary: Summary,
    pub cold_err: Option<String>,
    pub stats: Vec<VariantStat>,
}

pub struct UiaReport {
    pub create_ms: f64,
    pub timeouts_set: bool,
    pub results: Vec<TargetResult>,
}

const INTERACTIVE: [UIA_CONTROLTYPE_ID; 11] = [
    UIA_ButtonControlTypeId,
    UIA_EditControlTypeId,
    UIA_ComboBoxControlTypeId,
    UIA_CheckBoxControlTypeId,
    UIA_RadioButtonControlTypeId,
    UIA_MenuItemControlTypeId,
    UIA_ListItemControlTypeId,
    UIA_TabItemControlTypeId,
    UIA_HyperlinkControlTypeId,
    UIA_TreeItemControlTypeId,
    UIA_DataItemControlTypeId,
];

const PROPS: [UIA_PROPERTY_ID; 6] = [
    UIA_NamePropertyId,
    UIA_ControlTypePropertyId,
    UIA_AutomationIdPropertyId,
    UIA_BoundingRectanglePropertyId,
    UIA_IsEnabledPropertyId,
    UIA_IsOffscreenPropertyId,
];

const WALK_DEADLINE: Duration = Duration::from_secs(20);
const WALK_CAP: usize = 60_000;
const SLOW_RUN_MS: f64 = 8_000.0;

fn is_interactive(ct: i32) -> bool {
    INTERACTIVE.iter().any(|c| c.0 == ct)
}

fn variant_i32(v: i32) -> VARIANT {
    let mut var = VARIANT::default();
    unsafe {
        (*var.Anonymous.Anonymous).vt = VT_I4;
        (*var.Anonymous.Anonymous).Anonymous.lVal = v;
    }
    var
}

fn variant_bool(v: &VARIANT) -> bool {
    unsafe { v.Anonymous.Anonymous.vt == VT_BOOL && v.Anonymous.Anonymous.Anonymous.boolVal.0 != 0 }
}

fn tally(s: &mut Summary, name: usize, aid: usize, ct: i32, offscreen: bool, invoke: bool) {
    s.count += 1;
    s.text_chars += name + aid;
    if !offscreen {
        s.onscreen += 1;
        if is_interactive(ct) {
            s.interactive_onscreen += 1;
        }
    }
    if invoke {
        s.invokable += 1;
    }
}

unsafe fn read_current(el: &IUIAutomationElement, s: &mut Summary) {
    unsafe {
        let name = el.CurrentName().map(|b| b.to_string().len()).unwrap_or(0);
        let ct = el.CurrentControlType().map(|c| c.0).unwrap_or(0);
        let aid = el.CurrentAutomationId().map(|b| b.to_string().len()).unwrap_or(0);
        let rect = el.CurrentBoundingRectangle().unwrap_or_default();
        let _enabled = el.CurrentIsEnabled().map(|b| b.as_bool()).unwrap_or(false);
        let offscreen = el.CurrentIsOffscreen().map(|b| b.as_bool()).unwrap_or(true);
        std::hint::black_box(rect);
        tally(s, name, aid, ct, offscreen, false);
    }
}

unsafe fn read_cached(el: &IUIAutomationElement, s: &mut Summary, patterns: bool) {
    unsafe {
        let name = el.CachedName().map(|b| b.to_string().len()).unwrap_or(0);
        let ct = el.CachedControlType().map(|c| c.0).unwrap_or(0);
        let aid = el.CachedAutomationId().map(|b| b.to_string().len()).unwrap_or(0);
        let rect = el.CachedBoundingRectangle().unwrap_or_default();
        let _enabled = el.CachedIsEnabled().map(|b| b.as_bool()).unwrap_or(false);
        let offscreen = el.CachedIsOffscreen().map(|b| b.as_bool()).unwrap_or(true);
        let mut invoke = false;
        if patterns {
            invoke = el.GetCachedPropertyValue(UIA_IsInvokePatternAvailablePropertyId).map(|v| variant_bool(&v)).unwrap_or(false);
            let value = el.GetCachedPropertyValue(UIA_IsValuePatternAvailablePropertyId).map(|v| variant_bool(&v)).unwrap_or(false);
            std::hint::black_box(value);
        }
        std::hint::black_box(rect);
        tally(s, name, aid, ct, offscreen, invoke);
    }
}

struct Ctx {
    uia: IUIAutomation,
    walker: IUIAutomationTreeWalker,
    cv: IUIAutomationCondition,
    interactive: IUIAutomationCondition,
    cr_full: IUIAutomationCacheRequest,
    cr_none: IUIAutomationCacheRequest,
    cr_tree: IUIAutomationCacheRequest,
    cr_tree_inter: IUIAutomationCacheRequest,
}

impl Ctx {
    unsafe fn new(uia: IUIAutomation) -> windows::core::Result<Self> {
        unsafe {
            let walker = uia.ControlViewWalker()?;
            let cv = uia.ControlViewCondition()?;
            let mut conds: Vec<Option<IUIAutomationCondition>> = Vec::new();
            for ct in INTERACTIVE {
                conds.push(Some(uia.CreatePropertyCondition(UIA_ControlTypePropertyId, &variant_i32(ct.0))?));
            }
            let any_type = uia.CreateOrConditionFromNativeArray(&conds)?;
            let interactive = uia.CreateAndCondition(&cv, &any_type)?;
            let mk = |patterns: bool| -> windows::core::Result<IUIAutomationCacheRequest> {
                let cr = uia.CreateCacheRequest()?;
                for p in PROPS {
                    cr.AddProperty(p)?;
                }
                if patterns {
                    cr.AddProperty(UIA_IsInvokePatternAvailablePropertyId)?;
                    cr.AddProperty(UIA_IsValuePatternAvailablePropertyId)?;
                }
                Ok(cr)
            };
            let cr_full = mk(true)?;
            let cr_none = mk(true)?;
            cr_none.SetAutomationElementMode(AutomationElementMode_None)?;
            let cr_tree = mk(true)?;
            cr_tree.SetTreeScope(TreeScope_Subtree)?;
            cr_tree.SetTreeFilter(&cv)?;
            let cr_tree_inter = mk(true)?;
            cr_tree_inter.SetTreeScope(TreeScope_Subtree)?;
            cr_tree_inter.SetTreeFilter(&interactive)?;
            Ok(Ctx { uia, walker, cv, interactive, cr_full, cr_none, cr_tree, cr_tree_inter })
        }
    }

    unsafe fn root(&self, hwnd: isize) -> windows::core::Result<IUIAutomationElement> {
        unsafe { self.uia.ElementFromHandle(HWND(hwnd as *mut c_void)) }
    }

    unsafe fn walk(&self, hwnd: isize) -> windows::core::Result<Summary> {
        unsafe {
            let root = self.root(hwnd)?;
            let deadline = Instant::now() + WALK_DEADLINE;
            let mut s = Summary::default();
            let mut stack: Vec<IUIAutomationElement> = vec![root];
            let mut kids: Vec<IUIAutomationElement> = Vec::new();
            while let Some(el) = stack.pop() {
                read_current(&el, &mut s);
                if s.count >= WALK_CAP || Instant::now() > deadline {
                    s.truncated = true;
                    break;
                }
                kids.clear();
                let mut child = self.walker.GetFirstChildElement(&el).ok();
                while let Some(c) = child {
                    child = self.walker.GetNextSiblingElement(&c).ok();
                    kids.push(c);
                }
                stack.extend(kids.drain(..).rev());
            }
            Ok(s)
        }
    }

    unsafe fn find_all(&self, hwnd: isize, cond: &IUIAutomationCondition, cr: &IUIAutomationCacheRequest) -> windows::core::Result<Summary> {
        unsafe {
            let root = self.root(hwnd)?;
            let arr = root.FindAllBuildCache(TreeScope_Subtree, cond, cr)?;
            let n = arr.Length()?;
            let mut s = Summary::default();
            for i in 0..n {
                if let Ok(el) = arr.GetElement(i) {
                    read_cached(&el, &mut s, true);
                }
            }
            Ok(s)
        }
    }

    unsafe fn tree(&self, hwnd: isize, cr: &IUIAutomationCacheRequest) -> windows::core::Result<Summary> {
        unsafe {
            let root = self.root(hwnd)?;
            let cached = root.BuildUpdatedCache(cr)?;
            let mut s = Summary::default();
            let mut stack = vec![cached];
            while let Some(el) = stack.pop() {
                read_cached(&el, &mut s, true);
                if let Ok(children) = el.GetCachedChildren() {
                    let n = children.Length().unwrap_or(0);
                    for i in (0..n).rev() {
                        if let Ok(c) = children.GetElement(i) {
                            stack.push(c);
                        }
                    }
                }
            }
            Ok(s)
        }
    }

    unsafe fn floor(&self, hwnd: isize) -> windows::core::Result<Summary> {
        unsafe {
            let root = self.root(hwnd)?;
            let mut s = Summary::default();
            let name = root.CurrentName().map(|b| b.len()).unwrap_or(0);
            tally(&mut s, name, 0, 0, false, false);
            Ok(s)
        }
    }

    unsafe fn hit_test(&self, pt: (i32, i32)) -> windows::core::Result<Summary> {
        unsafe {
            let el = self.uia.ElementFromPointBuildCache(POINT { x: pt.0, y: pt.1 }, &self.cr_full)?;
            let mut s = Summary::default();
            read_cached(&el, &mut s, true);
            Ok(s)
        }
    }

    unsafe fn run_variant(&self, idx: usize, hwnd: isize) -> windows::core::Result<Summary> {
        unsafe {
            match idx {
                0 => self.walk(hwnd),
                1 => self.find_all(hwnd, &self.cv, &self.cr_full),
                2 => self.find_all(hwnd, &self.interactive, &self.cr_full),
                3 => self.tree(hwnd, &self.cr_tree),
                4 => self.find_all(hwnd, &self.cv, &self.cr_none),
                5 => self.tree(hwnd, &self.cr_tree_inter),
                _ => self.floor(hwnd),
            }
        }
    }
}

pub const VARIANT_NAMES: [&str; 8] = [
    "a) TreeWalker uncached",
    "b) FindAllBuildCache full",
    "c) FindAllBuildCache interactive",
    "d) BuildUpdatedCache tree",
    "e) FindAllBuildCache full, mode None",
    "f) BuildUpdatedCache tree interactive",
    "g) ElementFromHandle+Name (floor)",
    "h) ElementFromPointBuildCache (center)",
];

pub fn run_all(targets: Vec<Target>, runs: usize) -> Result<UiaReport, String> {
    std::thread::spawn(move || unsafe {
        let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
        if hr.is_err() {
            return Err(format!("CoInitializeEx MTA failed {hr:?}"));
        }
        let t = Instant::now();
        let uia: IUIAutomation = CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER).map_err(|e| err_str(&e))?;
        let create_ms = ms_since(t);
        let mut timeouts_set = false;
        if let Ok(u2) = uia.cast::<IUIAutomation2>() {
            timeouts_set = u2.SetConnectionTimeout(2_000).is_ok() && u2.SetTransactionTimeout(20_000).is_ok();
        }
        let ctx = Ctx::new(uia).map_err(|e| err_str(&e))?;
        let mut results = Vec::new();
        for target in targets {
            eprintln!("  UIA target: {} ({} / {})", target.label, target.process, target.class);
            let t = Instant::now();
            let cold = ctx.run_variant(2, target.hwnd);
            let cold_ms = ms_since(t);
            let (cold_summary, cold_err) = match cold {
                Ok(s) => (s, None),
                Err(e) => (Summary::default(), Some(err_str(&e))),
            };
            let mut stats: Vec<VariantStat> = VARIANT_NAMES.iter().map(|n| VariantStat { name: n, runs: vec![], summary: Summary::default(), err: None }).collect();
            for _ in 0..runs {
                for (i, st) in stats.iter_mut().enumerate() {
                    if st.err.is_some() || st.runs.iter().any(|&r| r > SLOW_RUN_MS) {
                        continue;
                    }
                    let t = Instant::now();
                    let r = if i == 7 { ctx.hit_test(target.center) } else { ctx.run_variant(i, target.hwnd) };
                    match r {
                        Ok(s) => {
                            st.runs.push(ms_since(t));
                            st.summary = s;
                        }
                        Err(e) => st.err = Some(err_str(&e)),
                    }
                }
            }
            results.push(TargetResult { target, cold_ms, cold_summary, cold_err, stats });
        }
        drop(ctx);
        CoUninitialize();
        Ok(UiaReport { create_ms, timeouts_set, results })
    })
    .join()
    .map_err(|_| "UIA thread panicked".to_string())?
}

pub fn probe_first_result() -> Result<(f64, f64, usize, f64, usize), String> {
    unsafe {
        let t = Instant::now();
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let uia: IUIAutomation = CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER).map_err(|e| err_str(&e))?;
        if let Ok(u2) = uia.cast::<IUIAutomation2>() {
            let _ = u2.SetConnectionTimeout(2_000);
            let _ = u2.SetTransactionTimeout(20_000);
        }
        let init_ms = ms_since(t);
        let fg = crate::enumwin::foreground();
        let root = uia.ElementFromHandle(fg).map_err(|e| err_str(&e))?;
        let name_len = root.CurrentName().map(|b| b.len()).unwrap_or(0);
        let _ = root.CurrentControlType();
        let first_ms = ms_since(t);
        let ctx = Ctx::new(uia).map_err(|e| err_str(&e))?;
        let s = ctx.run_variant(2, fg.0 as isize).map_err(|e| err_str(&e))?;
        let snap_ms = ms_since(t);
        Ok((init_ms, first_ms, name_len, snap_ms, s.count))
    }
}
