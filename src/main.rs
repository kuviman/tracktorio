use geng::prelude::*;

#[derive(Deserialize)]
struct Config {
    background: Rgba<f32>,
    fov: f32,
}

struct Game {
    geng: Geng,
    camera: Camera2d,
    config: Config,
}

impl Game {
    async fn new(geng: &Geng) -> Self {
        let config: Config = file::load_detect(run_dir().join("assets").join("config.toml"))
            .await
            .unwrap();
        Self {
            geng: geng.clone(),
            camera: Camera2d {
                center: vec2::ZERO,
                rotation: Angle::ZERO,
                fov: Camera2dFov::MinSide(config.fov),
            },
            config,
        }
    }
}

impl geng::State for Game {
    fn draw(&mut self, framebuffer: &mut ugli::Framebuffer) {
        ugli::clear(framebuffer, Some(self.config.background), None, None);
    }
}

fn main() {
    geng::setup_panic_handler();
    Geng::run("tracktorio", |geng| async move {
        geng.run_state(Game::new(&geng).await).await
    });
}
