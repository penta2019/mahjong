use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::model::*;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TenhouLog {
    pub log: Vec<Value>,
    pub name: [String; SEAT],
    pub rule: TenhouRule,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ratingc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lobby: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dan: Option<[String; SEAT]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate: Option<[f32; SEAT]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sx: Option<[String; SEAT]>,
}

impl TenhouLog {
    pub fn new() -> Self {
        let mut log = Self::default();
        log.rule.disp = "東喰赤".to_string();
        log.rule.aka = 1;
        log
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TenhouRule {
    pub disp: String,
    pub aka: usize,
    pub aka51: usize,
    pub aka52: usize,
    pub aka53: usize,
}

#[derive(Debug, Default)]
struct TenhouPlayer {
    hand: Vec<i64>,       // 配牌13枚 (親番14枚目はツモ扱い)
    drawns: Vec<Value>,   // 他家から鳴きで得た配を含む
    discards: Vec<Value>, // 捨て牌(ツモ切りの情報を含む)
}

#[derive(Debug, Default)]
struct TenhouKyoku {
    kyoku: usize,
    honba: usize,
    kyoutaku: usize,
    scores: [Score; SEAT],
    doras: Vec<i64>,
    ura_doras: Vec<i64>,
    players: [TenhouPlayer; SEAT],
    result: String,
    result_detail: Vec<Vec<Value>>,
}

impl TenhouKyoku {
    fn to_log(&self) -> Value {
        let mut v = vec![
            json!([self.kyoku, self.honba, self.kyoutaku]),
            json!(self.scores),
            json!(self.doras),
            json!(self.ura_doras),
        ];
        for p in &self.players {
            v.push(json!(p.hand));
            v.push(json!(p.drawns));
            v.push(json!(p.discards));
        }
        let mut result = vec![json!(self.result)];
        for d in &self.result_detail {
            result.push(json!(d));
        }
        v.push(json!(result));
        json!(v)
    }
}

// [TenhouSerializer]
#[derive(Debug)]
pub struct TenhouSerializer {
    log: TenhouLog,
    kyoku: TenhouKyoku,
}

impl TenhouSerializer {
    pub fn new(log: TenhouLog) -> Self {
        Self {
            log: log,
            kyoku: TenhouKyoku::default(),
        }
    }

    pub fn push_event(&mut self, stg: &Stage, event: &Event) {
        let k = &mut self.kyoku;
        match event {
            Event::Begin(_) => {}
            Event::New(e) => {
                self.kyoku = TenhouKyoku::default();
                let k = &mut self.kyoku;
                k.kyoku = e.bakaze * 4 + e.kyoku;
                k.honba = e.honba;
                k.kyoutaku = e.kyoutaku;
                k.doras = tiles_to_tenhou(&e.doras);
                k.scores = e.scores;
                for s in 0..SEAT {
                    let h = &e.hands[s];
                    k.players[s].hand = tiles_to_tenhou(&h[..13]);
                    if h.len() == 14 {
                        k.players[s].drawns.push(json!(tile_to_tenhou(h[13])));
                    }
                }
            }
            Event::Deal(e) => {
                k.players[e.seat].drawns.push(json!(tile_to_tenhou(e.tile)));
            }
            Event::Discard(e) => {
                let d = if e.is_drawn {
                    60
                } else {
                    tile_to_tenhou(e.tile)
                };
                let d = if e.is_riichi {
                    json!(format!("r{}", d))
                } else {
                    json!(d)
                };
                k.players[e.seat].discards.push(d);
            }
            Event::Meld(e) => match e.meld_type {
                MeldType::Chi | MeldType::Pon | MeldType::Minkan => {
                    let (seat, _, d) = stg.last_tile.unwrap();
                    let pos = 3 - (seat + SEAT - e.seat) % SEAT;
                    let marker = match e.meld_type {
                        MeldType::Chi => "c",
                        MeldType::Pon => "p",
                        MeldType::Minkan => "m",
                        _ => panic!(),
                    };
                    let mut meld: Vec<String> = e
                        .consumed
                        .iter()
                        .map(|&t| tile_to_tenhou(t).to_string())
                        .collect();
                    meld.insert(pos, format!("{}{}", marker, tile_to_tenhou(d)));
                    k.players[e.seat].drawns.push(json!(meld.concat()));
                }
                MeldType::Kakan => {
                    let t = e.consumed[0];
                    let tn = t.to_normal();
                    for m in &stg.players[e.seat].melds {
                        if m.tiles[0].to_normal() == tn {
                            let mut meld = "".to_string();
                            for i in 0..3 {
                                if m.froms[i] != e.seat {
                                    meld += "k";
                                    meld += &tile_to_tenhou(t).to_string();
                                }
                                meld += &tile_to_tenhou(m.tiles[i]).to_string();
                            }
                            k.players[e.seat].discards.push(json!(meld));
                        }
                    }
                }
                MeldType::Ankan => {
                    let mut meld: Vec<String> = e
                        .consumed
                        .iter()
                        .map(|&t| tile_to_tenhou(t).to_string())
                        .collect();
                    meld.insert(3, "a".to_string());
                    k.players[e.seat].discards.push(json!(meld.concat()));
                }
            },
            Event::Kita(_) => panic!(),
            Event::Dora(e) => {
                k.doras.push(tile_to_tenhou(e.tile));
            }
            Event::Win(e) => {
                let target_seat = stg.turn;
                k.result = "和了".to_string();
                k.ura_doras = tiles_to_tenhou(&e.ura_doras);
                for (seat, points, ctx) in &e.contexts {
                    k.result_detail
                        .push(points.iter().map(|&p| json!(p)).collect());
                    let mut detail = vec![json!(seat), json!(target_seat), json!(seat)];
                    let score_title = if ctx.score_title == "" {
                        format!("{}符{}飜", ctx.fu, ctx.fan)
                    } else {
                        match ctx.score_title.as_str() {
                            "数え役満" | "二倍役満" | "三倍役満" => "役満".to_string(),
                            _ => ctx.score_title.clone(),
                        }
                    };
                    if *seat == stg.turn {
                        if ctx.points.2 == 0 {
                            detail.push(json!(format!("{}{}点∀", score_title, ctx.points.1)));
                        } else {
                            detail.push(json!(format!(
                                "{}{}-{}点",
                                score_title, ctx.points.1, ctx.points.2,
                            )));
                        }
                    } else {
                        detail.push(json!(format!("{}{}点", score_title, ctx.points.0)));
                    }
                    for y in &ctx.yakus {
                        detail.push(json!(format!("{}({}飜)", y.0, y.1)));
                    }
                    k.result_detail.push(detail);
                }
            }
            Event::Draw(e) => {
                // TODO
                k.result = "流局".to_string();
                k.result_detail
                    .push(e.points.iter().map(|&p| json!(p)).collect());
            }
            Event::End(_) => {}
        }
    }

    pub fn serialize(&mut self) -> String {
        self.log.log = vec![self.kyoku.to_log()];
        serde_json::to_string(&self.log).unwrap()
    }
}

// [TenhouDeserializer]
// struct TenhouDeserializer {}

fn tile_to_tenhou(t: Tile) -> i64 {
    (match t {
        Z8 => 0,                            // Unknown
        Tile(ti, 0) => 50 + ti + 1,         // 赤ドラ
        Tile(ti, ni) => (ti + 1) * 10 + ni, // 通常
    }) as i64
}

// fn tile_from_tenhou(t: i64) -> Tile {
//     let t = t as usize;
//     match t {
//         0 => Z8,
//         11..=47 => Tile(t / 10 - 1, t % 10),
//         51..=53 => Tile(t % 10 - 1, 0),
//         _ => panic!("invalid tile number: {}", t),
//     }
// }

fn tiles_to_tenhou(v: &[Tile]) -> Vec<i64> {
    v.iter().map(|&t| tile_to_tenhou(t)).collect()
}

// fn tiles_from_tenhou(v: &[i64]) -> Vec<Tile> {
//     v.iter().map(|&t| tile_from_tenhou(t)).collect()
// }
