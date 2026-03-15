use crate::display::RefreshMode;
use crate::framebuffer::DisplayBuffers;
use crate::render_policy::RenderPolicy;

use super::runtime::InvalidationState;
use super::geom::Rect;

extern crate alloc;

use alloc::vec::Vec;

#[derive(Clone, Copy, Debug)]
pub struct RenderRequest {
    pub rect: Rect,
    pub refresh: RefreshMode,
}

#[derive(Default, Debug)]
pub struct RenderQueue {
    requests: Vec<RenderRequest>,
}

#[derive(Clone, Debug, Default)]
pub struct FlushSummary {
    pub request_count: usize,
    pub request_rects: Vec<Rect>,
    pub final_rect: Option<Rect>,
    pub refresh: Option<RefreshMode>,
}

impl RenderQueue {
    pub fn push(&mut self, rect: Rect, refresh: RefreshMode) {
        self.requests.push(RenderRequest { rect, refresh });
    }

    pub fn drain(&mut self) -> impl Iterator<Item = RenderRequest> + '_ {
        self.requests.drain(..)
    }

    pub fn is_empty(&self) -> bool {
        self.requests.is_empty()
    }
}

pub struct UiContext<'a> {
    pub buffers: &'a mut DisplayBuffers,
    pub render_policy: RenderPolicy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RenderLayer {
    Base,
    Overlay,
}

pub trait View {
    fn render(&mut self, ctx: &mut UiContext<'_>, rect: Rect, rq: &mut RenderQueue);

    fn layer(&self) -> RenderLayer {
        RenderLayer::Base
    }
}

pub struct PositionedView<'a> {
    pub rect: Rect,
    pub view: &'a mut dyn View,
}

pub fn render_positioned_views(
    ctx: &mut UiContext<'_>,
    rq: &mut RenderQueue,
    views: &mut [PositionedView<'_>],
) {
    for layer in [RenderLayer::Base, RenderLayer::Overlay] {
        for positioned in views.iter_mut() {
            if positioned.view.layer() == layer {
                positioned.view.render(ctx, positioned.rect, rq);
            }
        }
    }
}

pub fn flush_queue(
    display: &mut impl crate::display::Display,
    buffers: &mut DisplayBuffers,
    rq: &mut RenderQueue,
    fallback: RefreshMode,
) -> FlushSummary {
    flush_queue_tracked(display, buffers, rq, fallback, None)
}

pub fn flush_queue_tracked(
    display: &mut impl crate::display::Display,
    buffers: &mut DisplayBuffers,
    rq: &mut RenderQueue,
    fallback: RefreshMode,
    invalidation: Option<&mut InvalidationState>,
) -> FlushSummary {
    let mut mode = None;
    let mut rect = None;
    let mut summary = FlushSummary::default();
    for request in rq.drain() {
        summary.request_count += 1;
        summary.request_rects.push(request.rect);
        mode = Some(match mode {
            Some(current) => max_refresh(current, request.refresh),
            None => request.refresh,
        });
        rect = Some(match rect {
            Some(current) => union_rect(current, request.rect),
            None => request.rect,
        });
    }
    summary.final_rect = rect;
    summary.refresh = Some(mode.unwrap_or(fallback));
    if let Some(invalidation) = invalidation {
        invalidation.damage.clear_presented();
        for request in &summary.request_rects {
            invalidation.record_presented_rect(*request, summary.refresh.unwrap_or(fallback));
        }
        log::info!(
            "damage present requests={} semantic={} final={:?} refresh={:?}",
            summary.request_count,
            invalidation.damage.overlay_rects.len().saturating_sub(summary.request_count),
            summary.final_rect,
            summary.refresh
        );
        display.set_damage_overlay(invalidation.damage.overlay_rects.as_slice());
    } else {
        display.set_damage_overlay(&[]);
    }
    match rect {
        Some(rect) => display.display_region(buffers, rect, mode.unwrap_or(fallback)),
        None => display.display(buffers, mode.unwrap_or(fallback)),
    }
    summary
}

fn union_rect(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = (a.x + a.w).max(b.x + b.w);
    let y1 = (a.y + a.h).max(b.y + b.h);
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

fn max_refresh(a: RefreshMode, b: RefreshMode) -> RefreshMode {
    use RefreshMode::*;
    match (a, b) {
        (Full, _) | (_, Full) => Full,
        (Half, _) | (_, Half) => Half,
        _ => Fast,
    }
}
