use deckhand_core::{Rect, Result};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory1, IDXGIOutput};

use crate::convert::rect_from_win32;
use crate::monitor_list::{Monitor, enumerate_monitors, order_key};
use crate::output::{DxgiSource, Output};

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
        self.built && factory_current && self.monitors == monitors && !self.outputs.iter().any(Output::needs_rebuild)
    }

    fn rebuild(&mut self, monitors: Vec<Monitor>) {
        let factory = unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }.ok();
        let mut outputs = factory.as_ref().map(dxgi_outputs).unwrap_or_default();
        add_gdi_only_monitors(&mut outputs, &monitors);
        outputs.sort_by_key(|output| order_key(is_primary(&monitors, output.monitor_id()), output.rect()));
        *self = Self { factory, monitors, outputs, built: true };
    }
}

fn is_primary(monitors: &[Monitor], monitor_id: isize) -> bool {
    monitors.iter().any(|monitor| monitor.id == monitor_id && monitor.primary)
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

#[cfg(test)]
mod tests {
    use deckhand_core::Point;

    use super::*;

    fn monitor(id: isize, rect: Rect, primary: bool, orientation: u32) -> Monitor {
        Monitor { id, rect, primary, orientation }
    }

    #[test]
    fn live_displays_put_the_primary_first_and_stay_current() {
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

    #[test]
    fn a_rotation_alone_makes_the_displays_stale() {
        crate::dpi::make_thread_per_monitor_aware().unwrap();
        let mut displays = Displays::default();
        displays.refresh().unwrap();
        let mut rotated = displays.monitors.clone();
        rotated[0].orientation ^= 2;
        assert!(!displays.is_current(&rotated));
        assert!(displays.is_current(&displays.monitors.clone()));
    }

    #[test]
    fn primary_lookup_uses_the_monitor_flag() {
        let monitors =
            [monitor(1, Rect::new(-1920, 0, 1920, 1080), false, 0), monitor(2, Rect::new(0, 0, 10, 10), true, 2)];
        assert!(is_primary(&monitors, 2));
        assert!(!is_primary(&monitors, 1));
        assert!(!is_primary(&monitors, 3));
    }

    #[test]
    fn monitors_without_dxgi_get_gdi_outputs() {
        let monitors = [monitor(7, Rect::new(0, 0, 10, 10), true, 0)];
        let mut outputs = Vec::new();
        add_gdi_only_monitors(&mut outputs, &monitors);
        assert_eq!(outputs.len(), 1);
        assert_eq!(
            (outputs[0].monitor_id(), outputs[0].rect(), outputs[0].mode()),
            (7, Rect::new(0, 0, 10, 10), "gdi")
        );
    }
}
