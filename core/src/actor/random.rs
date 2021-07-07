use rand::Rng;

use super::*;

pub struct RandomDiscardBuilder;

impl ActorBuilder for RandomDiscardBuilder {
    fn get_default_config(&self) -> Config {
        Config {
            name: "RandomDiscard".to_string(),
            args: vec![],
        }
    }

    fn create(&self, config: Config) -> Box<dyn Actor> {
        Box::new(RandomDiscard::from_config(config))
    }
}

#[derive(Clone)]
pub struct RandomDiscard {
    config: Config,
    rng: rand::rngs::StdRng,
}

impl RandomDiscard {
    pub fn from_config(config: Config) -> Self {
        RandomDiscard {
            config: config,
            rng: rand::SeedableRng::seed_from_u64(0),
        }
    }
}

impl Actor for RandomDiscard {
    fn select_action(&mut self, stage: &Stage, seat: Seat, _acts: &Vec<Action>) -> Action {
        if stage.turn != seat {
            return Action::nop();
        }

        let h = &stage.players[seat].hand;
        let mut n: u32 = self.rng.gen_range(0..13);
        loop {
            for ti in 0..TYPE {
                for ni in 1..TNUM {
                    if h[ti][ni] > 0 {
                        if n == 0 {
                            return Action::discard(Tile(ti, ni));
                        }
                        n -= 1;
                    }
                }
            }
        }
    }

    fn get_config(&self) -> &Config {
        &self.config
    }
}

impl StageListener for RandomDiscard {}
