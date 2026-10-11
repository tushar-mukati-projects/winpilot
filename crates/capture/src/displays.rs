use deckhand_core::{Context, Point, Rect, Result};
use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory1, IDXGIOutput};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};
use windows_core::BOOL;

use crate::convert::rect_from_win32;
use crate::output::{DxgiSource, Output};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Monitor {
    id: isize,
    rect: Rect,
}

#[derive(Default)]
pub(crate) struct Displays {
    factory: Option<IDXGIFactory1>,
    monitors: Vec<Monitor>,
    outputs: Vec<Output>,
    built: bool,
}

impl Displays {
    pub(crate) fn refresh(&mut self) -> Result<()> {
        let monitors = enumerate_monitors()?;
        if !self.is_current(&monitors) {
            self.rebuild(monitors);
        }
        Ok(())
    }

    pub(crate) fn rects(&self) -> Vec<Rect> {
        self.outputs.iter().map(Output::rect).collect()
    }

    pub(crate) fn outputs_mut(&mut self) -> &mut [Output] {
        &mut self.outputs
    }

    #[cfg(test)]
    pub(crate) fn modes(&self) -> Vec<&'static str> {
        self.outputs.iter().map(Output::mode).collect()
    }

    fn is_current(&self, monitors: &[Monitor]) -> bool {
        let factory_current = self.factory.as_ref().is_none_or(|factory| unsafe { factory.IsCurrent() }.as_bool());
        self.built && factory_current && self.monitors == monitors && !self.outputs.iter().any(Output::access_lost)
    }

    fn rebuild(&mut self, monitors: Vec<Monitor>) {
        let factory = unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }.ok();
        let mut outputs = factory.as_ref().map(dxgi_outputs).unwrap_or_default();
        add_gdi_only_monitors(&mut outputs, &monitors);
        put_primary_first(&mut outputs, Output::rect);
        *self = Self { factory, monitors, outputs, built: true };
    }
}

fn dxgi_outputs(factory: &IDXGIFactory1) -> Vec<Output> {
    let mut outputs: Vec<Output> = Vec::new();
    for adapter in adapters(factory) {
        for output in adapter_outputs(&adapter) {
            let Some((monitor_id, rect)) = attached_area(&output) else {
                continue;
            };
            let source = DxgiSource { adapter: adapter.clone(), output };
            match outputs.iter_mut().find(|existing| existing.monitor_id() == monitor_id) {
                Some(existing) => existing.add_source(source),
                None => outputs.push(Output::new(monitor_id, rect, vec![source])),
            }
        }
    }
    outputs
}

fn adapters(factory: &IDXGIFactory1) -> impl Iterator<Item = IDXGIAdapter1> + '_ {
    (0..u32::MAX).map_while(move |index| unsafe { factory.EnumAdapters1(index) }.ok())
}

fn adapter_outputs(adapter: &IDXGIAdapter1) -> impl Iterator<Item = IDXGIOutput> + '_ {
    (0..u32::MAX).map_while(move |index| unsafe { adapter.EnumOutputs(index) }.ok())
}

fn attached_area(output: &IDXGIOutput) -> Option<(isize, Rect)> {
    let desc = unsafe { output.GetDesc() }.ok()?;
    desc.AttachedToDesktop.as_bool().then(|| (desc.Monitor.0 as isize, rect_from_win32(desc.DesktopCoordinates)))
}

fn add_gdi_only_monitors(outputs: &mut Vec<Output>, monitors: &[Monitor]) {
    for monitor in monitors {
        if !outputs.iter().any(|output| output.monitor_id() == monitor.id) {
            outputs.push(Output::new(monitor.id, monitor.rect, Vec::new()));
        }
    }
}

fn put_primary_first<T>(items: &mut Vec<T>, rect_of: impl Fn(&T) -> Rect) {
    let Some(index) = items.iter().position(|item| rect_of(item).contains(Point::new(0, 0))) else {
        return;
    };
    let primary = items.remove(index);
    items.insert(0, primary);
}

fn enumerate_monitors() -> Result<Vec<Monitor>> {
    let mut monitors: Vec<Monitor> = Vec::new();
    let collector = LPARAM(&raw mut monitors as isize);
    unsafe { EnumDisplayMonitors(None, None, Some(collect_monitor), collector) }.ok().context("EnumDisplayMonitors")?;
    Ok(monitors)
}

unsafe extern "system" fn collect_monitor(monitor: HMONITOR, _: HDC, area: *mut RECT, collector: LPARAM) -> BOOL {
    let monitors = unsafe { &mut *(collector.0 as *mut Vec<Monitor>) };
    if let Some(area) = unsafe { area.as_ref() } {
        monitors.push(Monitor { id: monitor.0 as isize, rect: rect_from_win32(*area) });
    }
    BOOL::from(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_moves_to_the_front_and_order_is_kept() {
        let mut rects =
            vec![Rect::new(-1920, 0, 1920, 1080), Rect::new(2560, 0, 1920, 1080), Rect::new(0, 0, 2560, 1600)];
        put_primary_first(&mut rects, |rect| *rect);
        assert_eq!(rects[0], Rect::new(0, 0, 2560, 1600));
        assert_eq!(rects[1], Rect::new(-1920, 0, 1920, 1080));
        assert_eq!(rects[2], Rect::new(2560, 0, 1920, 1080));
    }

    #[test]
    fn order_is_unchanged_without_a_primary() {
        let mut rects = vec![Rect::new(10, 10, 5, 5), Rect::new(20, 20, 5, 5)];
        put_primary_first(&mut rects, |rect| *rect);
        assert_eq!(rects, vec![Rect::new(10, 10, 5, 5), Rect::new(20, 20, 5, 5)]);
    }

    #[test]
    fn live_displays_cover_the_origin() {
        crate::dpi::make_thread_per_monitor_aware().unwrap();
        let mut displays = Displays::default();
        displays.refresh().unwrap();
        let rects = displays.rects();
        assert!(!rects.is_empty());
        assert!(rects[0].contains(Point::new(0, 0)));
        let monitors = enumerate_monitors().unwrap();
        assert_eq!(monitors.len(), rects.len());
        assert!(displays.is_current(&monitors));
    }
}
