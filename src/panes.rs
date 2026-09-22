//! Class-based pane layout for the main band (change `pane-class-layout`).

use crate::app::ViewType;

/// Window class of a view: the panes it may occupy and the right-half shape
/// its presence selects (design D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneClass {
    /// Full-height pane: the left pane, or the right pane when no small view
    /// is open.
    Big,
    /// Quarter-height pane of the split right half.
    Small,
}

/// The four panes of the adaptive band.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneId {
    BigLeft,
    BigRight,
    SmallTop,
    SmallBottom,
}

/// A pane: its fixed class and the view currently occupying it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pane {
    pub class: PaneClass,
    pub view: Option<ViewType>,
}

/// Adaptive band layout: a full-height left big pane plus a right half that
/// is one big pane or two small panes, decided by `has_small_view`.
#[derive(Debug, Clone, PartialEq)]
pub struct PaneLayout {
    pub big_left: Pane,
    pub big_right: Pane,
    pub small_top: Pane,
    pub small_bottom: Pane,
    /// Focused pane; `Tab`/`Shift+Tab` cycle it in tree order.
    pub focus: PaneId,
    /// Non-latching maximize: clears on `z`, on focus change, and on `Esc`.
    pub maximized: Option<PaneId>,
    pub main_split_ratio: f64,
    pub small_split_ratio: f64,
}

impl ViewType {
    /// Which pane class a view may occupy (spec "Window classes route views
    /// to panes").
    pub fn class(self) -> PaneClass {
        match self {
            ViewType::Graph | ViewType::Panels | ViewType::Physical => PaneClass::Big,
            ViewType::Optimizer | ViewType::SourceViewer => PaneClass::Small,
        }
    }
}

impl PaneLayout {
    pub fn pane(&self, id: PaneId) -> &Pane {
        match id {
            PaneId::BigLeft => &self.big_left,
            PaneId::BigRight => &self.big_right,
            PaneId::SmallTop => &self.small_top,
            PaneId::SmallBottom => &self.small_bottom,
        }
    }

    pub fn pane_mut(&mut self, id: PaneId) -> &mut Pane {
        match id {
            PaneId::BigLeft => &mut self.big_left,
            PaneId::BigRight => &mut self.big_right,
            PaneId::SmallTop => &mut self.small_top,
            PaneId::SmallBottom => &mut self.small_bottom,
        }
    }

    /// Whether a small-class view is open: the right half shows the two small
    /// panes instead of a single big pane.
    pub fn has_small_view(&self) -> bool {
        self.small_top.view.is_some() || self.small_bottom.view.is_some()
    }
}

impl Default for PaneLayout {
    /// Startup arrangement (spec "Startup pane configuration"): the module UI
    /// in the left big pane and the source viewer in a small pane.
    fn default() -> Self {
        PaneLayout {
            big_left: Pane {
                class: PaneClass::Big,
                view: Some(ViewType::Panels),
            },
            big_right: Pane {
                class: PaneClass::Big,
                view: None,
            },
            small_top: Pane {
                class: PaneClass::Small,
                view: Some(ViewType::SourceViewer),
            },
            small_bottom: Pane {
                class: PaneClass::Small,
                view: None,
            },
            focus: PaneId::BigLeft,
            maximized: None,
            main_split_ratio: 0.5,
            small_split_ratio: 0.5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_view_has_its_window_class() {
        assert_eq!(ViewType::Graph.class(), PaneClass::Big);
        assert_eq!(ViewType::Panels.class(), PaneClass::Big);
        assert_eq!(ViewType::Physical.class(), PaneClass::Big);
        assert_eq!(ViewType::Optimizer.class(), PaneClass::Small);
        assert_eq!(ViewType::SourceViewer.class(), PaneClass::Small);
    }

    #[test]
    fn default_layout_starts_in_small_arrangement() {
        let layout = PaneLayout::default();
        assert_eq!(layout.big_left.view, Some(ViewType::Panels));
        assert_eq!(layout.small_top.view, Some(ViewType::SourceViewer));
        assert_eq!(layout.big_right.view, None);
        assert_eq!(layout.small_bottom.view, None);
        assert_eq!(layout.focus, PaneId::BigLeft);
        assert!(layout.maximized.is_none());
        assert_eq!(layout.main_split_ratio, 0.5);
        assert_eq!(layout.small_split_ratio, 0.5);
        assert!(layout.has_small_view());
    }

    #[test]
    fn small_view_predicate_tracks_the_small_panes() {
        let mut layout = PaneLayout::default();
        layout.small_top.view = None;
        layout.small_bottom.view = None;
        assert!(!layout.has_small_view());
    }

    #[test]
    fn pane_accessors_route_by_id() {
        let mut layout = PaneLayout::default();
        assert_eq!(layout.pane(PaneId::BigLeft).class, PaneClass::Big);
        assert_eq!(layout.pane(PaneId::SmallTop).class, PaneClass::Small);
        layout.pane_mut(PaneId::SmallBottom).view = Some(ViewType::Optimizer);
        assert_eq!(
            layout.pane(PaneId::SmallBottom).view,
            Some(ViewType::Optimizer)
        );
    }
}
