use crate::{
    framebuffer::DisplayBuffers,
    palm::{form_preview::FormPreview, runtime::PalmFont},
    render_policy::RenderPolicy,
    ternos::ui::{Rect, StatusBarActionState, StatusBarView, UiContext, View},
};
use embedded_graphics::geometry::OriginDimensions;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrcStatusBarFocus {
    Home,
    Menu,
}

#[derive(Clone, Copy, Debug)]
pub struct PrcPaneLayout {
    pub content_top: i32,
    pub scale: i32,
    pub pane_x: i32,
    pub pane_y: i32,
    pub pane_w: i32,
    pub pane_h: i32,
}

pub fn active_form(
    runtime_form_id: Option<u16>,
    form_index: usize,
    forms: &[FormPreview],
) -> Option<FormPreview> {
    runtime_form_id
        .and_then(|fid| forms.iter().find(|f| f.form_id == fid).cloned())
        .or_else(|| forms.get(form_index).cloned())
        .or_else(|| forms.first().cloned())
}

pub fn pane_layout(screen_w: i32, screen_h: i32) -> PrcPaneLayout {
    let content_top = StatusBarView::HEIGHT + 5;
    let content_h = (screen_h - content_top).max(1);
    let max_scale_w = (screen_w / 160).max(1);
    let max_scale_h = (content_h / 160).max(1);
    let max_scale = max_scale_w.min(max_scale_h).max(1);
    let scale = if max_scale >= 3 { 3 } else { max_scale };
    let pane_w = 160 * scale;
    let pane_h = 160 * scale;
    let pane_x = ((screen_w - pane_w) / 2).max(0);
    let pane_y = content_top;
    PrcPaneLayout {
        content_top,
        scale: scale.max(1),
        pane_x,
        pane_y,
        pane_w,
        pane_h,
    }
}

pub fn render_status_bar(
    display_buffers: &mut DisplayBuffers,
    render_policy: RenderPolicy,
    palm_fonts: &[PalmFont],
    battery_percent: Option<u8>,
    shell_focus: Option<PrcStatusBarFocus>,
    menu_enabled: bool,
) {
    let size = display_buffers.size();
    let mut shell_ui = UiContext {
        buffers: display_buffers,
        render_policy,
        gray2: None,
    };
    let mut status_bar = StatusBarView::new(palm_fonts);
    status_bar.battery_percent = battery_percent;
    status_bar.home = StatusBarActionState {
        enabled: true,
        focused: shell_focus == Some(PrcStatusBarFocus::Home),
    };
    status_bar.menu = StatusBarActionState {
        enabled: menu_enabled,
        focused: shell_focus == Some(PrcStatusBarFocus::Menu),
    };
    status_bar.render(
        &mut shell_ui,
        Rect::new(0, 0, size.width as i32, StatusBarView::HEIGHT),
        &mut crate::ternos::ui::RenderQueue::default(),
    );
}
