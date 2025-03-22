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
    snap_distance: f32,
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
    FromNode { id: Id },
}

#[derive(Debug, Copy, Clone)]
enum Hover {
    Nothing { pos: vec2<f32> },
    Node { id: Id },
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash)]
struct Id(u64);

struct IdGen {
    next: u64,
}

impl IdGen {
    pub fn new() -> Self {
        Self { next: 0 }
    }
    pub fn gen(&mut self) -> Id {
        let id = Id(self.next);
        self.next += 1;
        id
    }
}

#[derive(HasId)]
struct TrackNode {
    id: Id,
    pos: vec2<f32>,
    connections: HashSet<Id>,
}

impl TrackNode {
    fn new(id_gen: &mut IdGen, pos: vec2<f32>) -> Self {
        Self {
            id: id_gen.gen(),
            pos,
            connections: HashSet::new(),
        }
    }
}

#[derive(Default)]
struct Tracks {
    nodes: Collection<TrackNode>,
}
impl Tracks {
    fn add_connection(&mut self, a: Id, b: Id) {
        self.nodes.get_mut(&a).unwrap().connections.insert(b);
        self.nodes.get_mut(&b).unwrap().connections.insert(a);
    }
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
    id_gen: IdGen,
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
            id_gen: IdGen::new(),
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
                        Hover::Node { id } => {
                            self.drawing = Some(Drawing::FromNode { id });
                        }
                    },
                    Some(drawing) => {
                        let start = match drawing {
                            Drawing::FromScratch { start } => {
                                let node = TrackNode::new(&mut self.id_gen, start);
                                let id = node.id;
                                self.tracks.nodes.insert(node);
                                id
                            }
                            Drawing::FromNode { id } => id,
                        };
                        let end = match self.hover {
                            Hover::Nothing { pos } => {
                                let node = TrackNode::new(&mut self.id_gen, pos);
                                let id = node.id;
                                self.tracks.nodes.insert(node);
                                id
                            }
                            Hover::Node { id } => id,
                        };
                        self.tracks.add_connection(start, end);
                        self.drawing = Some(Drawing::FromNode { id: end });
                    }
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
                if let Some(closest_node) = self
                    .tracks
                    .nodes
                    .iter()
                    .min_by_key(|node| r32((node.pos - cursor_world_pos).len()))
                {
                    if let Some(node_screen_pos) = self
                        .camera
                        .world_to_screen(self.framebuffer_size, closest_node.pos)
                    {
                        if (node_screen_pos - cursor_screen_position.map(|x| x as f32)).len()
                            * self.config.control.target_window_height
                            / self.framebuffer_size.y
                            < self.config.control.snap_distance
                        {
                            self.hover = Hover::Node {
                                id: closest_node.id,
                            };
                        }
                    }
                }
            }
            _ => {}
        }
    }
    fn draw(&mut self, framebuffer: &mut ugli::Framebuffer) {
        self.framebuffer_size = framebuffer.size().map(|x| x as f32);
        ugli::clear(framebuffer, Some(self.config.background), None, None);

        for a in &self.tracks.nodes {
            for b in &a.connections {
                let b = self.tracks.nodes.get(b).unwrap();
                if b.id.0 > a.id.0 {
                    continue;
                }
                self.geng.draw2d().draw2d(
                    framebuffer,
                    &self.camera,
                    &draw2d::Segment::new(
                        Segment(a.pos, b.pos),
                        self.config.track.width,
                        self.config.track.color,
                    ),
                );
            }
        }

        // preview
        if let Some(drawing) = self.drawing {
            let start = match drawing {
                Drawing::FromScratch { start } => start,
                Drawing::FromNode { id } => self.tracks.nodes.get(&id).unwrap().pos,
            };
            let end = match self.hover {
                Hover::Nothing { pos } => pos,
                Hover::Node { id } => self.tracks.nodes.get(&id).unwrap().pos,
            };
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
        match self.hover {
            Hover::Nothing { .. } => {}
            Hover::Node { id } => self.geng.draw2d().draw2d(
                framebuffer,
                &self.camera,
                &draw2d::Ellipse::circle(
                    self.tracks.nodes.get(&id).unwrap().pos,
                    self.config.track.width,
                    self.config.drawing.preview_color,
                ),
            ),
        }
    }
}

fn main() {
    geng::setup_panic_handler();
    Geng::run("tracktorio", |geng| async move {
        geng.run_state(Game::new(&geng).await).await
    });
}
