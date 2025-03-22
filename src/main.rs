use std::collections::VecDeque;

use geng::prelude::*;
use itertools::Itertools;

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
struct TrainConfig {
    width: f32,
    capacity: f32,
    color: Rgba<f32>,
}

#[derive(Deserialize)]
struct TestConfig {
    train_length: f32,
    train_speed: f32,
    train_load_speed: f32,
    text_color: Rgba<f32>,
    text_size: f32,
    amount_size: f32,
    amount_color: Rgba<f32>,
}

#[derive(Deserialize)]
struct FactoryIoConfig {
    r#type: IoType,
    resource: String,
    speed: Option<f32>,
}

#[derive(Deserialize)]
struct FactoryType {
    name: String,
    radius: f32,
    io: Vec<FactoryIoConfig>,
    color: Rgba<f32>,
}

#[derive(Deserialize)]
struct FactoryTypes {
    factory: Vec<FactoryType>,
}

impl FactoryTypes {
    fn get(&self, index: usize) -> Option<&FactoryType> {
        self.factory.get(index)
    }
}

impl Index<usize> for FactoryTypes {
    type Output = FactoryType;
    fn index(&self, index: usize) -> &Self::Output {
        &self.factory[index]
    }
}

#[derive(Deserialize)]
struct FactoryConfig {}

#[derive(Deserialize)]
struct StationConfig {
    radius: f32,
    color: Rgba<f32>,
}

#[derive(Deserialize)]
struct Config {
    station: StationConfig,
    background: Rgba<f32>,
    fov: FovConfig,
    track: TrackConfig,
    drawing: DrawingConfig,
    control: ControlConfig,
    test: TestConfig,
    train: TrainConfig,
    factory: FactoryConfig,
}

#[derive(Debug, Copy, Clone)]
enum Drawing {
    FromScratch { start: vec2<f32> },
    FromNode { id: Id },
}

#[derive(Debug, Copy, Clone)]
enum Hover {
    Nothing { pos: vec2<f32> },
    TrackNode { id: Id },
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

#[derive(Debug, Copy, Clone)]
struct TrackPoint {
    from: Id,
    to: Id,
    ratio: f32,
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
    fn point_pos(&self, point: TrackPoint) -> vec2<f32> {
        let from = self.nodes.get(&point.from).unwrap();
        let to = self.nodes.get(&point.to).unwrap();
        from.pos + (to.pos - from.pos) * point.ratio
    }

    fn segment_length(&self, from: Id, to: Id) -> f32 {
        let from = self.nodes.get(&from).unwrap();
        let to = self.nodes.get(&to).unwrap();
        (from.pos - to.pos).len()
    }

    fn pathfind(&self, from: Id, to: Id) -> Option<Vec<Id>> {
        let to = self.nodes.get(&to).unwrap();
        let (path, _cost) = pathfinding::directed::astar::astar(
            &from,
            |&v| {
                let v = self.nodes.get(&v).unwrap();
                v.connections.iter().copied().map(|u| {
                    let u = self.nodes.get(&u).unwrap();
                    (u.id, noisy_float::prelude::r32((v.pos - u.pos).len()))
                })
            },
            |id| noisy_float::prelude::r32((self.nodes.get(&id).unwrap().pos - to.pos).len()),
            |&v| v == to.id,
        )?;
        Some(path)
    }
}

#[derive(Deserialize, Copy, Clone, PartialEq, Eq, Hash)]
enum IoType {
    Input,
    Output,
}

struct FactoryIo {
    ty: IoType,
    node: Id,
    resource: Id,
    amount: Option<f32>,
    pos: vec2<f32>,
}

#[derive(HasId)]
struct Factory {
    id: Id,
    ty: usize,
    pos: vec2<f32>,
    io: Vec<FactoryIo>,
}

#[derive(HasId)]
struct Train {
    id: Id,
    resource: Id,
    amount: f32,
    length: f32,
    head: TrackPoint,
    tail_nodes: VecDeque<Id>,
    path_from_target: Option<Vec<Id>>,
    target: Option<IoId>,
}

#[derive(Copy, Clone, Debug)]
struct IoId {
    factory: Id,
    io: usize,
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

#[derive(HasId)]
struct Resource {
    id: Id,
    name: String,
}

struct Game {
    cursor_world_position: vec2<f32>,
    id_gen: IdGen,
    geng: Geng,
    framebuffer_size: vec2<f32>,
    camera: Camera2d,
    config: Config,
    factory_types: FactoryTypes,

    hover: Hover,
    drawing: Option<Drawing>,
    tracks: Tracks,
    trains: Collection<Train>,
    resources: Collection<Resource>,
    factories: Collection<Factory>,

    control: Control,
}

impl Game {
    async fn new(geng: &Geng) -> Self {
        let config: Config = file::load_detect(run_dir().join("assets").join("config.toml"))
            .await
            .unwrap();
        let factory_types: FactoryTypes =
            file::load_detect(run_dir().join("assets").join("factories.toml"))
                .await
                .unwrap();
        Self {
            cursor_world_position: vec2::ZERO,
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
            factory_types,

            tracks: Tracks::default(),
            trains: Collection::new(),
            control: Control::Idle,
            resources: default(),
            factories: default(),
        }
    }
    fn spawn_factory(&mut self, pos: vec2<f32>, angle: Angle<f32>, factory_type_index: usize) {
        let Some(factory_type) = self.factory_types.get(factory_type_index) else {
            return;
        };
        let factory = Factory {
            ty: factory_type_index,
            id: self.id_gen.gen(),
            pos,
            io: factory_type
                .io
                .iter()
                .enumerate()
                .map(|(index, io)| {
                    let io_pos = pos
                        + vec2(factory_type.radius, 0.0).rotate(
                            angle
                                + Angle::from_degrees(
                                    360.0 * index as f32 / factory_type.io.len() as f32,
                                ),
                        );
                    let node = TrackNode::new(&mut self.id_gen, io_pos);
                    let node_id = node.id;
                    self.tracks.nodes.insert(node);
                    FactoryIo {
                        ty: io.r#type,
                        node: node_id,
                        amount: io.speed.is_some().then_some(0.0),
                        resource: {
                            let existing = self
                                .resources
                                .iter()
                                .find(|resource| resource.name == io.resource);
                            if let Some(existing) = existing {
                                existing.id
                            } else {
                                let id = self.id_gen.gen();
                                self.resources.insert(Resource {
                                    id,
                                    name: io.resource.clone(),
                                });
                                id
                            }
                        },
                        pos: io_pos,
                    }
                })
                .collect(),
        };
        self.factories.insert(factory);
    }
    fn spawn_train(&mut self) {
        let Some(resource) = self.resources.iter().choose(&mut thread_rng()) else {
            return;
        };
        if let Some(node) = self.tracks.nodes.iter().choose(&mut thread_rng()) {
            let id = self.id_gen.gen();
            let train = Train {
                target: None,
                id,
                length: self.config.test.train_length,
                resource: resource.id,
                amount: 0.0,
                head: TrackPoint {
                    from: node.id,
                    to: node.id,
                    ratio: 0.0,
                },
                tail_nodes: default(),
                path_from_target: None,
            };
            self.trains.insert(train);
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

        for factory in &mut self.factories {
            let factory_type = &self.factory_types[factory.ty];
            let mut max_input_dt = delta_time;
            for (io, io_config) in factory.io.iter().zip(&factory_type.io) {
                if io.ty == IoType::Input {
                    if let (Some(amount), Some(speed)) = (io.amount, io_config.speed) {
                        max_input_dt = max_input_dt.min(amount / speed);
                    }
                }
            }
            for (io, io_config) in factory.io.iter_mut().zip(&factory_type.io) {
                match io.ty {
                    IoType::Input => {
                        if let (Some(amount), Some(speed)) = (&mut io.amount, io_config.speed) {
                            *amount = (*amount - speed * max_input_dt).max(0.0);
                        }
                    }
                    IoType::Output => {
                        if let (Some(amount), Some(speed)) = (&mut io.amount, io_config.speed) {
                            *amount += speed * max_input_dt;
                        }
                    }
                }
            }
        }

        for train in &mut self.trains {
            if train.path_from_target.is_some() {
                continue;
            }
            let mut go = false;
            if let Some(io) = train.target {
                let io = &mut self.factories.get_mut(&io.factory).unwrap().io[io.io];
                match io.ty {
                    IoType::Input => {
                        let unload_amount = train
                            .amount
                            .min(self.config.test.train_load_speed * delta_time);
                        train.amount -= unload_amount;
                        if let Some(io_amount) = &mut io.amount {
                            *io_amount += unload_amount;
                        }
                        if train.amount.approx_eq(&0.0) {
                            go = true;
                        }
                    }
                    IoType::Output => {
                        let mut load_amount = (self.config.train.capacity - train.amount)
                            .min(self.config.test.train_load_speed * delta_time);
                        if let Some(io_amount) = io.amount {
                            load_amount = load_amount.min(io_amount);
                        }
                        train.amount += load_amount;
                        if let Some(io_amount) = &mut io.amount {
                            *io_amount -= load_amount;
                        }
                        if self.config.train.capacity.approx_eq(&train.amount) {
                            go = true;
                        }
                    }
                }
            } else {
                go = true;
            }

            if go {
                let look_for = if train.amount > self.config.train.capacity / 2.0 {
                    IoType::Input
                } else {
                    IoType::Output
                };
                let target = self
                    .factories
                    .iter()
                    .flat_map(|factory| {
                        factory
                            .io
                            .iter()
                            .enumerate()
                            .map(|(index, io)| (factory.id, index, io))
                    })
                    .filter(|(_, _, io)| io.ty == look_for && io.resource == train.resource)
                    .choose(&mut thread_rng());
                if let Some((factory_id, io_index, io)) = target {
                    train.path_from_target = self.tracks.pathfind(io.node, train.head.to);
                    if train.path_from_target.is_some() {
                        train.target = Some(IoId {
                            factory: factory_id,
                            io: io_index,
                        })
                    }
                }
            }
        }

        for train in &mut self.trains {
            let Some(path) = &mut train.path_from_target else {
                continue;
            };
            while path.last() == Some(&train.head.to) {
                path.pop();
            }
            let from = self.tracks.nodes.get(&train.head.from).unwrap();
            let to = self.tracks.nodes.get(&train.head.to).unwrap();
            let current_segment_length = self.tracks.segment_length(from.id, to.id);
            let mut current_segment_progress = train.head.ratio * current_segment_length;
            current_segment_progress += self.config.test.train_speed * delta_time;
            if current_segment_progress < current_segment_length {
                train.head.ratio = current_segment_progress / current_segment_length;
            } else if let Some(next_node) = path.pop() {
                let next_node = self.tracks.nodes.get(&next_node).unwrap();
                let next_segment_length = self.tracks.segment_length(to.id, next_node.id);
                let next_segment_progress = current_segment_progress - current_segment_length;
                train.head = TrackPoint {
                    from: to.id,
                    to: next_node.id,
                    ratio: next_segment_progress / next_segment_length,
                };
                train.tail_nodes.push_front(to.id);

                let mut covered_length = next_segment_progress;
                for (i, (a, b)) in train.tail_nodes.iter().copied().tuple_windows().enumerate() {
                    if covered_length > train.length {
                        train.tail_nodes.truncate(i + 1);
                        break;
                    }
                    covered_length += self.tracks.segment_length(a, b);
                }
            } else {
                train.head.ratio = 1.0;
                train.path_from_target = None;
            }
        }
    }
    fn handle_event(&mut self, event: geng::Event) {
        match event {
            geng::Event::KeyPress { key } => match key {
                geng::Key::Space => {
                    self.spawn_train();
                }
                geng::Key::Digit0 => {
                    self.spawn_factory(self.cursor_world_position, thread_rng().gen(), 0);
                }
                geng::Key::Digit1 => {
                    self.spawn_factory(self.cursor_world_position, thread_rng().gen(), 1);
                }
                geng::Key::Digit2 => {
                    self.spawn_factory(self.cursor_world_position, thread_rng().gen(), 2);
                }
                geng::Key::Digit3 => {
                    self.spawn_factory(self.cursor_world_position, thread_rng().gen(), 3);
                }
                geng::Key::Digit4 => {
                    self.spawn_factory(self.cursor_world_position, thread_rng().gen(), 4);
                }
                _ => {}
            },
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
                        Hover::TrackNode { id } => {
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
                            Hover::TrackNode { id } => id,
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
                self.cursor_world_position = cursor_world_pos;
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
                            self.hover = Hover::TrackNode {
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

        for factory in &self.factories {
            let factory_type = &self.factory_types[factory.ty];
            self.geng.draw2d().draw2d(
                framebuffer,
                &self.camera,
                &draw2d::Ellipse::circle(factory.pos, factory_type.radius, factory_type.color),
            );
            self.geng.draw2d().draw2d(
                framebuffer,
                &self.camera,
                &draw2d::Text::unit(
                    &**self.geng.default_font(),
                    &factory_type.name,
                    self.config.test.text_color,
                )
                .fit_into(Ellipse::circle(factory.pos, factory_type.radius)),
            );

            for io in &factory.io {
                let resource = self.resources.get(&io.resource).unwrap();
                self.geng.draw2d().draw2d(
                    framebuffer,
                    &self.camera,
                    &draw2d::Ellipse::circle(
                        io.pos,
                        self.config.station.radius,
                        self.config.station.color,
                    ),
                );
                self.geng.draw2d().draw2d(
                    framebuffer,
                    &self.camera,
                    &draw2d::Text::unit(
                        &**self.geng.default_font(),
                        &resource.name,
                        self.config.test.text_color,
                    )
                    .align_bounding_box(vec2(0.5, 0.0))
                    .transform(
                        mat3::translate(io.pos) * mat3::scale_uniform(self.config.test.text_size),
                    ),
                );
                if let Some(amount) = io.amount {
                    self.geng.default_font().draw(
                        framebuffer,
                        &self.camera,
                        &format!("{amount:.1}"),
                        vec2(geng::TextAlign::CENTER, geng::TextAlign::BOTTOM),
                        mat3::translate(io.pos) * mat3::scale_uniform(self.config.test.amount_size),
                        self.config.test.amount_color,
                    );
                }
            }
        }

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

        for train in &self.trains {
            let mut pos = self.tracks.point_pos(train.head);
            let mut draw_towards = |to_pos: vec2<f32>| {
                self.geng.draw2d().draw2d(
                    framebuffer,
                    &self.camera,
                    &draw2d::Segment::new(
                        Segment(pos, to_pos),
                        self.config.train.width,
                        self.config.train.color,
                    ),
                );
                pos = to_pos;
            };

            let mut node = train.head.to;
            let mut covered_length =
                self.tracks.segment_length(train.head.from, train.head.to) * train.head.ratio;
            let last_node = 'last: {
                for (a, b) in train.tail_nodes.iter().copied().tuple_windows() {
                    if covered_length > train.length {
                        break 'last Some(a);
                    }
                    covered_length += self.tracks.segment_length(a, b);
                    draw_towards(self.tracks.nodes.get(&a).unwrap().pos);
                    node = a;
                }
                train.tail_nodes.back().copied()
            };
            if let Some(last_node) = last_node {
                let segment_length = self.tracks.segment_length(last_node, node);
                draw_towards(self.tracks.point_pos(TrackPoint {
                    from: last_node,
                    to: node,
                    ratio: (covered_length - train.length).max(0.0) / segment_length,
                }));
            }
            self.geng.default_font().draw(
                framebuffer,
                &self.camera,
                &format!("{:.1}", train.amount),
                vec2(geng::TextAlign::CENTER, geng::TextAlign::BOTTOM),
                mat3::translate(self.tracks.point_pos(train.head))
                    * mat3::scale_uniform(self.config.test.amount_size),
                self.config.test.amount_color,
            );
        }

        // preview
        if let Some(drawing) = self.drawing {
            let start = match drawing {
                Drawing::FromScratch { start } => start,
                Drawing::FromNode { id } => self.tracks.nodes.get(&id).unwrap().pos,
            };
            let end = match self.hover {
                Hover::Nothing { pos } => pos,
                Hover::TrackNode { id } => self.tracks.nodes.get(&id).unwrap().pos,
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
            Hover::TrackNode { id } => self.geng.draw2d().draw2d(
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
