use eframe::egui::{self, Pos2, Vec2};
use fish_s2pro_tts::{audio::AudioPlayer, timeline::{render_timeline_page, TimelineClip, TimelineState}};

fn draw(ctx: &egui::Context, state: &mut TimelineState, size: Vec2, events: Vec<egui::Event>) -> egui::FullOutput {
    ctx.run(egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, size)),
        events,
        ..Default::default()
    }, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            render_timeline_page(ui, state, &[], "", &mut AudioPlayer::new_headless());
        });
    })
}

fn title_pos(output: &egui::FullOutput, prefix: &str) -> Pos2 {
    output.shapes.iter().find_map(|s| {
        if let egui::Shape::Text(t) = &s.shape {
            if t.galley.text().starts_with(prefix) && s.clip_rect.contains(t.pos) {
                return Some(t.pos);
            }
        }
        None
    }).unwrap_or_else(|| panic!("visible text not found: {prefix}"))
}

#[test]
fn timeline_renders_stacked_tracks_and_drags_clip_in_real_egui_frames() {
    for size in [Vec2::new(860.0, 600.0), Vec2::new(1400.0, 1000.0)] {
        let ctx = egui::Context::default();
        let mut state = TimelineState::new();
        state.tracks[0].name = "TRACK_A".into();
        state.tracks[1].name = "TRACK_B".into();
        state.clips.push(TimelineClip::new_mock(99, 0, "CLIP_TEST".into(), "".into(), "".into(), 1.0, 2.0, [40, 180, 130]));
        for _ in 0..3 { draw(&ctx, &mut state, size, vec![]); }
        let out = draw(&ctx, &mut state, size, vec![]);
        let a = title_pos(&out, "TRACK_A");
        let b = title_pos(&out, "TRACK_B");
        let clip = title_pos(&out, "CLIP_TEST");
        assert!((a.x - b.x).abs() < 1.0);
        assert!((b.y - a.y - state.tracks[0].height - 4.0).abs() < 1.0);
        assert!(clip.x > a.x + 190.0 && clip.x < size.x);
        let from = clip + Vec2::new(25.0, 38.0);
        draw(&ctx, &mut state, size, vec![egui::Event::PointerMoved(from), egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() }]);
        let to = from + Vec2::new(80.0, b.y - a.y);
        draw(&ctx, &mut state, size, vec![egui::Event::PointerMoved(to)]);
        draw(&ctx, &mut state, size, vec![egui::Event::PointerButton { pos: to, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() }]);
        assert!((state.clips[0].start_sec - 2.0).abs() < 0.02, "start was {}", state.clips[0].start_sec);
        assert_eq!(state.clips[0].track_id, 1);
    }
}
