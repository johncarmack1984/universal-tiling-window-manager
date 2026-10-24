//! Every action in John's export, on this machine's display: 1920×1080 with a
//! 48-pixel taskbar, so a 1920×1032 work area. Expected rects are worked out
//! from Rectangle's formulas by hand, so a regression in the port shows up as a
//! pixel diff here.

use utwm_config::settings::Settings;
use utwm_config::{ActionId, Anchor, Dimension, ManualSpec, Placement, Target};
use utwm_layout::{Engine, Move, PixelRect, Plan, Request, Screen};

const WORK: PixelRect = PixelRect::new(0, 0, 1920, 1032);
const START: PixelRect = PixelRect::new(100, 100, 900, 700);

fn screen() -> Screen {
    Screen {
        frame: PixelRect::new(0, 0, 1920, 1080),
        work: WORK,
        scale: 1.0,
        primary: true,
    }
}

/// John's export: cycle sizes ½ → ⅔ → ⅓, no gaps, 10% minimum sizes.
fn settings() -> Settings {
    Settings {
        minimum_window_width: 0.1,
        minimum_window_height: 0.1,
        ..Settings::default()
    }
}

/// One simulated window: presses go through the engine and the window lands
/// exactly where it was sent, as a cooperative app does.
struct Window {
    engine: Engine<u32>,
    frame: PixelRect,
    screens: Vec<Screen>,
    settings: Settings,
    specs: Vec<ManualSpec>,
    others: Vec<PixelRect>,
}

impl Window {
    fn new(frame: PixelRect) -> Self {
        Self {
            engine: Engine::new(),
            frame,
            screens: vec![screen()],
            settings: settings(),
            specs: Vec::new(),
            others: Vec::new(),
        }
    }

    fn plan(&mut self, target: Target) -> Plan {
        let request = Request {
            target,
            window: self.frame,
            screens: &self.screens,
            settings: &self.settings,
            specs: &self.specs,
            others: &self.others,
        };
        self.engine.plan(&1, &request)
    }

    /// Presses `action` and returns where the window ends up.
    fn press(&mut self, action: ActionId) -> PixelRect {
        self.press_target(Target::Action(action))
    }

    fn press_target(&mut self, target: Target) -> PixelRect {
        match self.plan(target) {
            Plan::Move(planned) => self.land(&planned),
            Plan::AlreadyThere => {}
            Plan::Nothing(why) => panic!("{target:?} did nothing: {why}"),
        }
        self.frame
    }

    /// Presses a chord bound to several actions, as the app does.
    fn press_group(&mut self, group: &[Target]) -> PixelRect {
        let target = self
            .engine
            .select(&1, group, self.frame)
            .expect("non-empty group");
        self.press_target(target)
    }

    fn land(&mut self, planned: &Move) {
        self.frame = planned.rect;
        self.engine.landed(&1, planned, planned.rect);
    }
}

fn rect(left: i32, top: i32, right: i32, bottom: i32) -> PixelRect {
    PixelRect::new(left, top, right, bottom)
}

#[test]
fn halves_cycle_one_half_two_thirds_one_third() {
    let mut w = Window::new(START);
    assert_eq!(w.press(ActionId::LeftHalf), rect(0, 0, 960, 1032));
    assert_eq!(w.press(ActionId::LeftHalf), rect(0, 0, 1280, 1032));
    assert_eq!(w.press(ActionId::LeftHalf), rect(0, 0, 640, 1032));
    assert_eq!(w.press(ActionId::LeftHalf), rect(0, 0, 960, 1032));
    assert_eq!(w.press(ActionId::RightHalf), rect(960, 0, 1920, 1032));
    assert_eq!(w.press(ActionId::RightHalf), rect(640, 0, 1920, 1032));
    assert_eq!(w.press(ActionId::TopHalf), rect(0, 0, 1920, 516));
    assert_eq!(w.press(ActionId::TopHalf), rect(0, 0, 1920, 688));
    assert_eq!(w.press(ActionId::BottomHalf), rect(0, 516, 1920, 1032));
}

#[test]
fn moving_a_window_by_hand_restarts_the_cycle() {
    let mut w = Window::new(START);
    assert_eq!(w.press(ActionId::LeftHalf), rect(0, 0, 960, 1032));
    w.frame = rect(10, 10, 970, 1042); // dragged
    assert_eq!(w.press(ActionId::LeftHalf), rect(0, 0, 960, 1032));
}

#[test]
fn corners_cycle_their_width() {
    let mut w = Window::new(START);
    assert_eq!(w.press(ActionId::TopLeft), rect(0, 0, 960, 516));
    assert_eq!(w.press(ActionId::TopLeft), rect(0, 0, 1280, 516));
    assert_eq!(w.press(ActionId::TopRight), rect(960, 0, 1920, 516));
    assert_eq!(w.press(ActionId::BottomLeft), rect(0, 516, 960, 1032));
    assert_eq!(w.press(ActionId::BottomRight), rect(960, 516, 1920, 1032));
    assert_eq!(w.press(ActionId::BottomRight), rect(640, 516, 1920, 1032));
}

#[test]
fn thirds_walk_and_two_thirds_toggle() {
    let mut w = Window::new(START);
    assert_eq!(w.press(ActionId::FirstThird), rect(0, 0, 640, 1032));
    assert_eq!(w.press(ActionId::FirstThird), rect(640, 0, 1280, 1032));
    assert_eq!(w.press(ActionId::FirstThird), rect(1280, 0, 1920, 1032));
    assert_eq!(w.press(ActionId::FirstThird), rect(0, 0, 640, 1032));
    assert_eq!(w.press(ActionId::CenterThird), rect(640, 0, 1280, 1032));
    assert_eq!(w.press(ActionId::LastThird), rect(1280, 0, 1920, 1032));
    assert_eq!(w.press(ActionId::LastThird), rect(640, 0, 1280, 1032));
    assert_eq!(w.press(ActionId::LastThird), rect(0, 0, 640, 1032));
    assert_eq!(w.press(ActionId::FirstTwoThirds), rect(0, 0, 1280, 1032));
    assert_eq!(w.press(ActionId::FirstTwoThirds), rect(640, 0, 1920, 1032));
    assert_eq!(w.press(ActionId::CenterTwoThirds), rect(320, 0, 1600, 1032));
    assert_eq!(w.press(ActionId::LastTwoThirds), rect(640, 0, 1920, 1032));
    assert_eq!(w.press(ActionId::LastTwoThirds), rect(0, 0, 1280, 1032));
}

#[test]
fn fourths_walk_and_the_middle_ones_grow() {
    let mut w = Window::new(START);
    assert_eq!(w.press(ActionId::FirstFourth), rect(0, 0, 480, 1032));
    assert_eq!(w.press(ActionId::FirstFourth), rect(480, 0, 960, 1032));
    assert_eq!(w.press(ActionId::FirstFourth), rect(960, 0, 1440, 1032));
    assert_eq!(w.press(ActionId::FirstFourth), rect(1440, 0, 1920, 1032));
    assert_eq!(w.press(ActionId::FirstFourth), rect(0, 0, 480, 1032));
    assert_eq!(w.press(ActionId::SecondFourth), rect(480, 0, 960, 1032));
    assert_eq!(w.press(ActionId::SecondFourth), rect(480, 0, 1920, 1032));
    assert_eq!(w.press(ActionId::SecondFourth), rect(480, 0, 1440, 1032));
    assert_eq!(w.press(ActionId::SecondFourth), rect(480, 0, 960, 1032));
    assert_eq!(w.press(ActionId::ThirdFourth), rect(960, 0, 1440, 1032));
    assert_eq!(w.press(ActionId::ThirdFourth), rect(0, 0, 1440, 1032));
    assert_eq!(w.press(ActionId::LastFourth), rect(1440, 0, 1920, 1032));
    assert_eq!(w.press(ActionId::LastFourth), rect(960, 0, 1440, 1032));
}

#[test]
fn sixths_on_the_keypad() {
    let mut w = Window::new(START);
    assert_eq!(w.press(ActionId::TopLeftSixth), rect(0, 0, 640, 516));
    assert_eq!(w.press(ActionId::TopLeftSixth), rect(640, 0, 1280, 516));
    assert_eq!(w.press(ActionId::TopLeftSixth), rect(1280, 0, 1920, 516));
    assert_eq!(w.press(ActionId::TopLeftSixth), rect(0, 516, 640, 1032));
    assert_eq!(w.press(ActionId::TopRightSixth), rect(1280, 0, 1920, 516));
    assert_eq!(w.press(ActionId::TopRightSixth), rect(640, 0, 1280, 516));
    assert_eq!(w.press(ActionId::BottomLeftSixth), rect(0, 516, 640, 1032));
    assert_eq!(
        w.press(ActionId::BottomRightSixth),
        rect(1280, 516, 1920, 1032)
    );
    // The center sixths grow into two sixths instead of walking.
    assert_eq!(w.press(ActionId::TopCenterSixth), rect(640, 0, 1280, 516));
    assert_eq!(w.press(ActionId::TopCenterSixth), rect(640, 0, 1920, 516));
    assert_eq!(w.press(ActionId::TopCenterSixth), rect(0, 0, 1280, 516));
    assert_eq!(w.press(ActionId::TopCenterSixth), rect(640, 0, 1280, 516));
    assert_eq!(
        w.press(ActionId::BottomCenterSixth),
        rect(640, 516, 1280, 1032)
    );
    assert_eq!(
        w.press(ActionId::BottomCenterSixth),
        rect(640, 516, 1920, 1032)
    );
    assert_eq!(
        w.press(ActionId::BottomCenterSixth),
        rect(0, 516, 1280, 1032)
    );
}

#[test]
fn eighths_on_the_keypad() {
    let mut w = Window::new(START);
    assert_eq!(
        w.press(ActionId::TopCenterLeftEighth),
        rect(480, 0, 960, 516)
    );
    assert_eq!(
        w.press(ActionId::TopCenterRightEighth),
        rect(960, 0, 1440, 516)
    );
    assert_eq!(w.press(ActionId::TopRightEighth), rect(1440, 0, 1920, 516));
    assert_eq!(w.press(ActionId::BottomLeftEighth), rect(0, 516, 480, 1032));
    assert_eq!(
        w.press(ActionId::BottomLeftEighth),
        rect(480, 516, 960, 1032)
    );
    assert_eq!(
        w.press(ActionId::BottomCenterLeftEighth),
        rect(480, 516, 960, 1032)
    );
    assert_eq!(
        w.press(ActionId::BottomCenterRightEighth),
        rect(960, 516, 1440, 1032)
    );
    assert_eq!(
        w.press(ActionId::BottomRightEighth),
        rect(1440, 516, 1920, 1032)
    );
    assert_eq!(w.press(ActionId::BottomRightEighth), rect(0, 0, 480, 516));
}

#[test]
fn maximize_center_and_maximize_height() {
    let mut w = Window::new(START);
    assert_eq!(w.press(ActionId::MaximizeHeight), rect(100, 0, 900, 1032));
    let mut w = Window::new(START);
    assert_eq!(w.press(ActionId::Center), rect(560, 216, 1360, 816));
    assert_eq!(w.press(ActionId::Maximize), WORK);
    // 90% of 1032 is 929, leaving 103 pixels: Rectangle rounds the margin in
    // y-up space, so the odd pixel goes to the bottom, not the top.
    assert_eq!(w.press(ActionId::AlmostMaximize), rect(96, 51, 1824, 980));
}

#[test]
fn larger_and_smaller_step_30_from_the_middle() {
    let mut w = Window::new(START);
    assert_eq!(w.press(ActionId::Larger), rect(85, 85, 915, 715));
    let mut w = Window::new(START);
    assert_eq!(w.press(ActionId::Smaller), rect(115, 115, 885, 685));
}

#[test]
fn smaller_stops_at_the_minimum_size() {
    // 10% of 1920 is 192 and 10% of 1032 is 103; 200×110 shrinks to 170×80,
    // which is too small, so the window stays put.
    let tiny = rect(500, 500, 700, 610);
    let mut w = Window::new(tiny);
    assert!(matches!(
        w.plan(Target::Action(ActionId::Smaller)),
        Plan::AlreadyThere
    ));
}

#[test]
fn larger_keeps_a_window_against_the_screen_edge() {
    let mut w = Window::new(rect(960, 0, 1920, 1032));
    // Against the right, top and bottom edges: it grows leftward only and the
    // height stays the full screen.
    assert_eq!(w.press(ActionId::Larger), rect(930, 0, 1920, 1032));
}

#[test]
fn restore_returns_to_the_position_before_snapping() {
    let mut w = Window::new(START);
    w.press(ActionId::LeftHalf);
    w.press(ActionId::LeftHalf);
    assert_eq!(w.press(ActionId::Restore), START);
}

#[test]
fn next_display_needs_a_second_display() {
    let mut w = Window::new(START);
    assert!(matches!(
        w.plan(Target::Action(ActionId::NextDisplay)),
        Plan::Nothing(_)
    ));
}

#[test]
fn next_display_keeps_the_edges_a_window_was_against() {
    let mut w = Window::new(rect(0, 0, 960, 1032));
    let big = Screen {
        frame: rect(1920, 0, 4480, 1440),
        work: rect(1920, 0, 4480, 1440),
        scale: 1.0,
        primary: false,
    };
    w.screens.push(big);
    // A left half arrives as a full-height window against the left edge.
    assert_eq!(w.press(ActionId::NextDisplay), rect(1920, 0, 2880, 1440));
    // A maximized window stays maximized.
    let mut w = Window::new(WORK);
    w.screens.push(big);
    assert_eq!(w.press(ActionId::NextDisplay), big.work);
    // Previous from the right display comes back.
    assert_eq!(w.press(ActionId::PreviousDisplay), WORK);
}

#[test]
fn two_thirds_then_fill_on_one_chord() {
    let group = [
        Target::Action(ActionId::FirstTwoThirds),
        Target::Action(ActionId::FillLeft),
    ];
    let mut w = Window::new(START);
    w.others = vec![rect(960, 0, 1920, 1032)];
    assert_eq!(w.press_group(&group), rect(0, 0, 1280, 1032));
    assert_eq!(w.press_group(&group), rect(0, 0, 960, 1032));
    assert_eq!(w.press_group(&group), rect(0, 0, 1280, 1032));
    // Moved by hand: the cycle starts over at two thirds.
    w.frame = START;
    assert_eq!(w.press_group(&group), rect(0, 0, 1280, 1032));
}

#[test]
fn fill_falls_back_to_the_half() {
    let mut w = Window::new(START);
    w.others = vec![rect(0, 0, 1900, 1032)];
    assert_eq!(w.press(ActionId::FillLeft), rect(0, 0, 960, 1032));
    assert_eq!(w.press(ActionId::FillRight), rect(960, 0, 1920, 1032));
}

#[test]
fn custom_position_below_the_movie() {
    let mut w = Window::new(START);
    w.specs = vec![ManualSpec {
        id: 33,
        name: "Left 1/3 Below Movie".to_owned(),
        shortcut: None,
        placement: Placement {
            anchor: Anchor::BottomLeft,
            x: Dimension::Keep,
            y: Dimension::Keep,
            width: Dimension::Fraction(0.333),
            height: Dimension::Fraction(0.7),
            display: Some(0),
        },
        repetitions: Vec::new(),
    }];
    assert_eq!(w.press_target(Target::Spec(33)), rect(0, 310, 639, 1032));
}

#[test]
fn tile_two_by_three_fills_rows_front_to_back() {
    let mut engine: Engine<u32> = Engine::new();
    let windows = [
        (1, START),
        (2, rect(50, 50, 400, 400)),
        (3, rect(700, 300, 1500, 900)),
    ];
    let placed = engine.plan_tile(2, 3, &windows, &[screen()], &settings());
    assert_eq!(
        placed,
        [
            (1, rect(0, 0, 640, 516)),
            (2, rect(640, 0, 1280, 516)),
            (3, rect(1280, 0, 1920, 516)),
        ]
    );
}

#[test]
fn gaps_leave_one_gap_between_halves() {
    let mut w = Window::new(START);
    w.settings.gap_size = 10.0;
    assert_eq!(w.press(ActionId::LeftHalf), rect(10, 10, 955, 1022));
    let mut w = Window::new(START);
    w.settings.gap_size = 10.0;
    assert_eq!(w.press(ActionId::RightHalf), rect(965, 10, 1910, 1022));
}
