use geng::prelude::*;

struct Game {
    geng: Geng,
}

impl Game {
    async fn new(geng: &Geng) -> Self {
        Self { geng: geng.clone() }
    }
}

impl geng::State for Game {
    fn draw(&mut self, framebuffer: &mut ugli::Framebuffer) {
        ugli::clear(framebuffer, Some(Rgba::BLACK), None, None);
    }
}

fn main() {
    geng::setup_panic_handler();
    Geng::run("tracktorio", |geng| async move {
        geng.run_state(Game::new(&geng).await).await
    });
}
