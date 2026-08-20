use serde::{Deserialize, Serialize};

/// Represents a loaded DROID patch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Patch {
    pub name: String,
    pub hw_components: Vec<HwComponent>,
    pub shift_groups: Vec<ShiftGroup>,
}

/// A hardware component from the patch (button, CV in/out, knob, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HwComponent {
    pub id: String,
    pub label: String,
    pub kind: ComponentKind,
    pub shift_group: Option<ShiftGroup>,
    pub state: ComponentState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComponentKind {
    Button,
    CvIn,
    CvOut,
    Knob,
    Switch,
    Led,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComponentState {
    Off,
    On,
    Value(f32),
    Active,
}

/// Shift groups — modifier keys that change the behavior/label of a group of components
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShiftGroup {
    Group1,
    Group2,
    Group3,
    Group4,
}

impl ShiftGroup {
    pub fn key_label(&self) -> &'static str {
        match self {
            ShiftGroup::Group1 => "1",
            ShiftGroup::Group2 => "2",
            ShiftGroup::Group3 => "3",
            ShiftGroup::Group4 => "4",
        }
    }

    pub fn color(&self) -> ratatui::style::Color {
        match self {
            ShiftGroup::Group1 => ratatui::style::Color::Yellow,
            ShiftGroup::Group2 => ratatui::style::Color::Cyan,
            ShiftGroup::Group3 => ratatui::style::Color::Magenta,
            ShiftGroup::Group4 => ratatui::style::Color::Green,
        }
    }
}

impl Patch {
    /// Sample patch for development/testing
    pub fn sample() -> Self {
        Self {
            name: String::from("Demo Patch"),
            hw_components: vec![
                HwComponent {
                    id: "btn_1".into(),
                    label: "TRIG A".into(),
                    kind: ComponentKind::Button,
                    shift_group: Some(ShiftGroup::Group1),
                    state: ComponentState::Off,
                },
                HwComponent {
                    id: "btn_2".into(),
                    label: "TRIG B".into(),
                    kind: ComponentKind::Button,
                    shift_group: Some(ShiftGroup::Group1),
                    state: ComponentState::Off,
                },
                HwComponent {
                    id: "cv_in_1".into(),
                    label: "CV IN 1".into(),
                    kind: ComponentKind::CvIn,
                    shift_group: Some(ShiftGroup::Group2),
                    state: ComponentState::Value(0.0),
                },
                HwComponent {
                    id: "cv_out_1".into(),
                    label: "CV OUT 1".into(),
                    kind: ComponentKind::CvOut,
                    shift_group: Some(ShiftGroup::Group2),
                    state: ComponentState::Value(0.0),
                },
                HwComponent {
                    id: "knob_1".into(),
                    label: "CUTOFF".into(),
                    kind: ComponentKind::Knob,
                    shift_group: Some(ShiftGroup::Group3),
                    state: ComponentState::Value(0.5),
                },
                HwComponent {
                    id: "led_1".into(),
                    label: "STATUS".into(),
                    kind: ComponentKind::Led,
                    shift_group: None,
                    state: ComponentState::On,
                },
            ],
            shift_groups: vec![
                ShiftGroup::Group1,
                ShiftGroup::Group2,
                ShiftGroup::Group3,
                ShiftGroup::Group4,
            ],
        }
    }

    /// Get components belonging to a specific shift group
    pub fn components_in_group(&self, group: ShiftGroup) -> Vec<&HwComponent> {
        self.hw_components
            .iter()
            .filter(|c| c.shift_group == Some(group))
            .collect()
    }
}
