use geng::prelude::*;

#[derive(Deserialize)]
struct DrawingConfig {
    preview_color: Rgba<f32>,
}

#[derive(Deserialize)]
struct ControlConfig {
    target_window_height: f32,
    min_drag_distance: f32,
    zoom_speed: f32,
    drag_timer: f64,
}

#[derive(Deserialize)]
struct TrackConfig {
    width: f32,
    color: Rgba<f32>,
}

#[derive(Deserialize)]
struct FovConfig {
    default: f32,
    min: f32,
    max: f32,
}

#[derive(Deserialize)]
struct Config {
    background: Rgba<f32>,
    fov: FovConfig,
    track: TrackConfig,
    drawing: DrawingConfig,
    control: ControlConfig,
}

#[derive(Debug, Copy, Clone)]
enum Drawing {
    FromScratch { start: vec2<f32> },
}

#[derive(Debug, Copy, Clone)]
enum Hover {
    Nothing { pos: vec2<f32> },
}

#[derive(Default)]
struct Tracks {
    segments: Vec<[vec2<f32>; 2]>,
}

enum Control {
    Idle,
    Detecting {
        start_world_pos: vec2<f32>,
        start_screen_pos: vec2<f64>,
        start_hover: Hover,
        timer: Timer,
    },
    MovingCamera {
        prev_pos: vec2<f32>,
    },
}

struct Game {
    geng: Geng,
    framebuffer_size: vec2<f32>,
    camera: Camera2d,
    config: Config,

    hover: Hover,
    drawing: Option<Drawing>,
    tracks: Tracks,

    control: Control,
}

impl Game {
    async fn new(geng: &Geng) -> Self {
        let config: Config = file::load_detect(run_dir().join("assets").join("config.toml"))
            .await
            .unwrap();
        Self {
            geng: geng.clone(),
            framebuffer_size: vec2::splat(1.0),
            camera: Camera2d {
                center: vec2::ZERO,
                rotation: Angle::ZERO,
                fov: Camera2dFov::MinSide(config.fov.default),
            },
            config,
            drawing: None,
            hover: Hover::Nothing { pos: vec2::ZERO },

            tracks: Tracks::default(),
            control: Control::Idle,
        }
    }
}

impl geng::State for Game {
    fn update(&mut self, delta_time: f64) {
        let delta_time = delta_time as f32;
        if let Control::Detecting {
            start_world_pos,
            ref timer,
            ..
        } = self.control
        {
            if timer.elapsed().as_secs_f64() > self.config.control.drag_timer {
                self.control = Control::MovingCamera {
                    prev_pos: start_world_pos,
                };
            }
        }
    }
    fn handle_event(&mut self, event: geng::Event) {
        match event {
            geng::Event::MousePress {
                button: geng::MouseButton::Left,
            } => {
                let position = self.geng.window().cursor_position().unwrap_or(vec2::ZERO);
                let world_pos = self
                    .camera
                    .screen_to_world(self.framebuffer_size, position.map(|x| x as f32));
                self.control = Control::Detecting {
                    start_world_pos: world_pos,
                    start_screen_pos: position,
                    start_hover: self.hover,
                    timer: Timer::new(),
                }
            }
            geng::Event::MouseRelease {
                button: geng::MouseButton::Left,
            } => match mem::replace(&mut self.control, Control::Idle) {
                Control::Idle => {}
                Control::MovingCamera { prev_pos: _ } => {}
                Control::Detecting { start_hover, .. } => match self.drawing {
                    None => match start_hover {
                        Hover::Nothing { pos } => {
                            self.drawing = Some(Drawing::FromScratch { start: pos })
                        }
                    },
                    Some(drawing) => match drawing {
                        Drawing::FromScratch { start } => match self.hover {
                            Hover::Nothing { pos } => {
                                let end = pos;
                                self.drawing = Some(Drawing::FromScratch { start: end });
                                self.tracks.segments.push([start, end]);
                            }
                        },
                    },
                },
            },
            geng::Event::Wheel { delta } => {
                let fov = self.camera.fov.value_mut();
                *fov = (*fov * self.config.control.zoom_speed.powf(-delta as f32))
                    .clamp(self.config.fov.min, self.config.fov.max);
            }
            geng::Event::MousePress {
                button: geng::MouseButton::Right,
            } => {
                self.drawing = None;
            }
            geng::Event::CursorMove {
                position: cursor_screen_position,
            } => {
                let cursor_world_pos = self.camera.screen_to_world(
                    self.framebuffer_size,
                    cursor_screen_position.map(|x| x as f32),
                );
                if let Control::Detecting {
                    start_world_pos,
                    start_screen_pos,
                    start_hover: _,
                    timer: _,
                } = self.control
                {
                    if (cursor_screen_position - start_screen_pos).len() as f32
                        * self.config.control.target_window_height
                        / self.framebuffer_size.y
                        > self.config.control.min_drag_distance
                    {
                        self.control = Control::MovingCamera {
                            prev_pos: start_world_pos,
                        }
                    }
                }
                if let Control::MovingCamera { prev_pos } = &mut self.control {
                    self.camera.center += *prev_pos - cursor_world_pos;
                }
                self.hover = Hover::Nothing {
                    pos: cursor_world_pos,
                };
            }
            _ => {}
        }
    }
    fn draw(&mut self, framebuffer: &mut ugli::Framebuffer) {
        self.framebuffer_size = framebuffer.size().map(|x| x as f32);
        ugli::clear(framebuffer, Some(self.config.background), None, None);

        for &[a, b] in &self.tracks.segments {
            self.geng.draw2d().draw2d(
                framebuffer,
                &self.camera,
                &draw2d::Segment::new(
                    Segment(a, b),
                    self.config.track.width,
                    self.config.track.color,
                ),
            );
        }

        if let Some(drawing) = self.drawing {
            match drawing {
                Drawing::FromScratch { start } => match self.hover {
                    Hover::Nothing { pos: end } => {
                        self.geng.draw2d().draw2d(
                            framebuffer,
                            &self.camera,
                            &draw2d::Segment::new(
                                Segment(start, end),
                                self.config.track.width,
                                self.config.drawing.preview_color,
                            ),
                        );
                    }
                },
            }
        }
    }
}

fn main() {
    geng::setup_panic_handler();
    Geng::run("tracktorio", |geng| async move {
        geng.run_state(Game::new(&geng).await).await
    });
}
