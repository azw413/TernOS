use crate::display::RefreshMode;
use crate::framebuffer::DisplayBuffers;
use crate::render_policy::RenderPolicy;

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

pub trait View {
    fn render(&mut self, ctx: &mut UiContext<'_>, rect: Rect, rq: &mut RenderQueue);
}

pub fn flush_queue(
    display: &mut impl crate::display::Display,
    buffers: &mut DisplayBuffers,
    rq: &mut RenderQueue,
    fallback: RefreshMode,
) {
    let mut mode = None;
    let mut rect = None;
    for request in rq.drain() {
        mode = Some(match mode {
            Some(current) => max_refresh(current, request.refresh),
            None => request.refresh,
        });
        rect = Some(match rect {
            Some(current) => union_rect(current, request.rect),
            None => request.rect,
        });
    }
    match rect {
        Some(rect) => display.display_region(buffers, rect, mode.unwrap_or(fallback)),
        None => display.display(buffers, mode.unwrap_or(fallback)),
    }
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
