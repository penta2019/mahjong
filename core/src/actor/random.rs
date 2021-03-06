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
    seat: Seat,
}

impl RandomDiscard {
    pub fn from_config(config: Config) -> Self {
        RandomDiscard {
            config: config,
            rng: rand::SeedableRng::seed_from_u64(0),
            seat: NO_SEAT,
        }
    }
}

impl Actor for RandomDiscard {
    fn init(&mut self, seat: Seat) {
        self.seat = seat;
    }

    fn select_action(&mut self, stage: &Stage, _acts: &Vec<Action>) -> Action {
        if stage.turn != self.seat {
            return Action::nop();
        }

        let pl = &stage.players[self.seat];
        let mut n: usize = self.rng.gen_range(0..13);
        loop {
            for ti in 0..TYPE {
                for ni in 1..TNUM {
                    let t = Tile(ti, ni);
                    let c = pl.count_tile(t);
                    if c > n {
                        return Action::discard(Tile(ti, ni));
                    } else {
                        n -= c;
                    }
                }
            }
        }
    }

    fn get_config(&self) -> &Config {
        &self.config
    }
}

impl Listener for RandomDiscard {}
