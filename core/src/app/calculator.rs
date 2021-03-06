use std::fs::File;
use std::io::{self, BufRead};

use crate::hand::{evaluate_hand, YakuFlags};
use crate::model::*;
use crate::util::common::*;

use crate::error;

#[derive(Debug)]
pub struct CalculatorApp {
    args: Vec<String>,
    detail: bool,
}

impl CalculatorApp {
    pub fn new(args: Vec<String>) -> Self {
        Self {
            args,
            detail: false,
        }
    }

    pub fn run(&mut self) {
        let mut file_path = "".to_string();
        let mut exp = "".to_string();
        let mut it = self.args.iter();
        while let Some(s) = it.next() {
            match s.as_str() {
                "-d" => self.detail = true,
                "-f" => file_path = next_value(&mut it, "-f"),
                _ => {
                    if exp.starts_with("-") {
                        error!("unknown option: {}", s);
                        return;
                    }
                    if exp != "" {
                        error!("multiple expression is not allowed");
                        return;
                    }
                    exp = s.clone();
                }
            }
        }

        if (file_path == "" && exp == "") || (file_path != "" && exp != "") {
            print_usage();
            return;
        }

        if exp != "" {
            if let Err(e) = self.process_expression(&exp) {
                error!("{}", e);
                return;
            }
        }

        if file_path != "" {
            if let Err(e) = self.run_from_file(&file_path) {
                error!("{}", e);
                return;
            }
        }
    }

    fn run_from_file(&self, file_path: &str) -> std::io::Result<()> {
        let file = File::open(file_path)?;
        let lines = io::BufReader::new(file).lines();
        for line in lines {
            if let Ok(exp) = line {
                let e = exp.replace(" ", "");
                if e == "" || e.chars().next().unwrap() == '#' {
                    // 空行とコメント行はスキップ
                    println!("> {}", exp);
                } else if let Err(e) = self.process_expression(&exp) {
                    error!("{}", e);
                }
                println!();
            }
        }
        Ok(())
    }

    fn process_expression(&self, exp: &str) -> Result<(), String> {
        let mut calculator = Calculator::new(self.detail);
        calculator.parse(exp)?;
        calculator.run()?;
        Ok(())
    }
}

#[derive(Debug)]
struct Calculator {
    detail: bool,
    seat: Seat,
    kyoku: usize,
    // evaluate_hand params
    hand: TileTable,
    melds: Vec<Meld>,
    doras: Vec<Tile>,
    ura_doras: Vec<Tile>,
    win_tile: Tile,
    is_drawn: bool,
    is_dealer: bool,
    prevalent_wind: Index,
    seat_wind: Index,
    yaku_flags: YakuFlags,
    // score verify
    fu: usize,
    fan: usize,
    score: i32,
}

impl Calculator {
    fn new(detail: bool) -> Self {
        Self {
            detail: detail,
            seat: 0,
            kyoku: 0,
            hand: TileTable::default(),
            melds: vec![],
            doras: vec![],
            ura_doras: vec![],
            win_tile: Z8,
            is_drawn: true,
            is_dealer: true,
            prevalent_wind: 1,
            seat_wind: 1,
            yaku_flags: YakuFlags::default(),
            fu: 0,
            fan: 0,
            score: 0,
        }
    }

    fn parse(&mut self, input: &str) -> Result<(), String> {
        println!("> {}", input);

        let input = input.replace(" ", "");
        let input = input.split("#").collect::<Vec<&str>>()[0]; // コメント削除
        let exps: Vec<&str> = input.split("/").collect();

        if let Some(exp) = exps.get(1) {
            self.parse_stage_info(exp)?; // 副露のパースに座席情報が必要なので最初に実行
        };
        if let Some(exp) = exps.get(0) {
            self.parse_hand_meld(exp)?;
        };
        if let Some(exp) = exps.get(2) {
            self.parse_yaku_flags(exp)?;
        };
        if let Some(exp) = exps.get(3) {
            self.parse_score_verify(exp)?;
        }

        if self.detail {
            println!("{:?}", self);
        }

        Ok(())
    }

    fn run(&self) -> Result<(), String> {
        if let Some(ctx) = evaluate_hand(
            &self.hand,
            &self.melds,
            &self.doras,
            &self.ura_doras,
            self.win_tile,
            self.is_drawn,
            self.is_dealer,
            self.prevalent_wind,
            self.seat_wind,
            &self.yaku_flags,
        ) {
            if self.detail {
                println!("{:?}", ctx);
            }

            let mut yakus = "".to_string();
            for y in ctx.yakus {
                yakus += &format!("{}: {}, ", y.0, y.1);
            }
            println!("yakus: {}", yakus);

            let score = if self.is_drawn {
                if self.is_dealer {
                    ctx.points.1 * 3
                } else {
                    ctx.points.1 * 2 + ctx.points.2
                }
            } else {
                ctx.points.0
            };
            println!(
                "fu: {}, fan: {}, score: {}, {}",
                ctx.fu, ctx.fan, score, ctx.score_title
            );

            let verify = if self.score != 0 {
                if ctx.yakuman_times > 0 {
                    // 役満以上は得点のみをチェック
                    if score == self.score {
                        "ok"
                    } else {
                        "error"
                    }
                } else {
                    if ctx.fu == self.fu && ctx.fan == self.fan && score == self.score {
                        "ok"
                    } else {
                        "error"
                    }
                }
            } else {
                "skip"
            };
            println!("verify: {}", verify);
        } else {
            error!("not win hand");
        }

        Ok(())
    }

    fn parse_stage_info(&mut self, input: &str) -> Result<(), String> {
        let exps: Vec<&str> = input.split(",").collect();
        if let Some(exp) = exps.get(0) {
            let chars: Vec<char> = exp.chars().collect();
            if chars.len() != 3 {
                return Err(format!("stage info len is not 3: {}", exp));
            }
            let prevalent_wind = wind_from_char(chars[0])?;
            let kyoku = chars[1].to_digit(10).unwrap() as usize;
            let seat_wind = wind_from_char(chars[2])?;

            if !(1 <= kyoku && kyoku <= 4) {
                return Err(format!("kyoku is not 1, 2, 3 or 4: {}", kyoku));
            }

            self.prevalent_wind = prevalent_wind;
            self.seat_wind = seat_wind;
            self.kyoku = kyoku - 1;
            self.is_dealer = seat_wind == 1;
            self.seat = (seat_wind + kyoku - 2) % SEAT;
        }
        if let Some(exp) = exps.get(1) {
            self.doras = tiles_from_string(exp)?;
        }
        if let Some(exp) = exps.get(2) {
            self.ura_doras = tiles_from_string(exp)?;
        }
        Ok(())
    }

    fn parse_hand_meld(&mut self, input: &str) -> Result<(), String> {
        let mut exp_hand = "".to_string();
        let mut exp_melds = vec![];
        for exp in input.split(',') {
            if exp_hand == "" {
                if exp.chars().last().unwrap() == '+' {
                    self.is_drawn = false;
                }
                exp_hand = exp.replace("+", "");
            } else {
                exp_melds.push(exp.to_string());
            }
        }

        // parse hands
        for t in tiles_from_string(&exp_hand)? {
            self.hand[t.0][t.1] += 1;
            if t.1 == 0 {
                self.hand[t.0][5] += 1;
            }
            self.win_tile = t;
        }

        // parse melds
        for exp_meld in &exp_melds {
            self.melds.push(meld_from_string(exp_meld, self.seat)?);
        }

        if self.is_drawn {
            self.yaku_flags.menzentsumo = true;
            for m in &self.melds {
                if m.type_ != MeldType::Ankan {
                    self.yaku_flags.menzentsumo = false;
                }
            }
        }

        Ok(())
    }

    fn parse_yaku_flags(&mut self, input: &str) -> Result<(), String> {
        for y in input.split(",") {
            match y {
                "立直" => self.yaku_flags.riichi = true,
                "両立直" => self.yaku_flags.dabururiichi = true,
                "一発" => self.yaku_flags.ippatsu = true,
                "海底摸月" => self.yaku_flags.haiteiraoyue = true,
                "河底撈魚" => self.yaku_flags.houteiraoyui = true,
                "嶺上開花" => self.yaku_flags.rinshankaihou = true,
                "槍槓" => self.yaku_flags.chankan = true,
                "天和" => self.yaku_flags.tenhou = true,
                "地和" => self.yaku_flags.tiihou = true,
                "" => {}
                _ => return Err(format!("invalid conditional yaku: {}", y)),
            }
        }
        Ok(())
    }

    fn parse_score_verify(&mut self, input: &str) -> Result<(), String> {
        let exps: Vec<&str> = input.split(",").collect();
        if exps.len() != 3 {
            return Err(format!("invalid score verify info: {}", input));
        }
        self.fu = exps[0].parse::<usize>().map_err(|e| e.to_string())?;
        self.fan = exps[1].parse::<usize>().map_err(|e| e.to_string())?;
        self.score = exps[2].parse::<i32>().map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn tiles_from_string(exp: &str) -> Result<Vec<Tile>, String> {
    let mut tiles = vec![];
    let undef: usize = 255;
    let mut ti = undef;
    for c in exp.chars() {
        match c {
            'm' => ti = 0,
            'p' => ti = 1,
            's' => ti = 2,
            'z' => ti = 3,
            '0'..='9' => {
                if ti == undef {
                    return Err(format!("tile number befor tile type"));
                }
                let ni = c.to_digit(10).unwrap() as usize;
                tiles.push(Tile(ti, ni));
            }
            _ => {
                return Err(format!("invalid char: '{}'", c));
            }
        }
    }
    Ok(tiles)
}

fn meld_from_string(exp: &str, seat: Seat) -> Result<Meld, String> {
    let undef: usize = 255;
    let mut ti = undef;
    let mut nis = vec![];
    let mut from = 0;
    let mut tiles = vec![];
    let mut froms = vec![];
    for c in exp.chars() {
        match c {
            'm' => ti = 0,
            'p' => ti = 1,
            's' => ti = 2,
            'z' => ti = 3,
            '+' => {
                if froms.is_empty() {
                    return Err("invalid '+' suffix".into());
                }
                let last = froms.len() - 1;
                froms[last] = from % SEAT;
            }
            '0'..='9' => {
                if ti == undef {
                    return Err("tile number befor tile type".into());
                }

                from += 1;
                let ni = c.to_digit(10).unwrap() as usize;
                nis.push(if ni == 0 { 5 } else { ni });
                tiles.push(Tile(ti, ni));
                froms.push(seat);
            }
            _ => {
                return Err(format!("invalid char: '{}'", c));
            }
        }
    }

    nis.sort();
    let mut diffs = vec![];
    let mut ni0 = nis[0];
    for ni in &nis[1..] {
        diffs.push(ni - ni0);
        ni0 = *ni;
    }

    let meld_type = if diffs.len() == 2 && vec_count(&diffs, &1) == 2 {
        MeldType::Chi
    } else if diffs.len() == 2 && vec_count(&diffs, &0) == 2 {
        MeldType::Pon
    } else if diffs.len() == 3 && vec_count(&diffs, &0) == 3 {
        if vec_count(&froms, &seat) == 4 {
            MeldType::Ankan
        } else {
            MeldType::Minkan
        }
    } else {
        return Err(format!("invalid meld: '{}'", exp));
    };

    Ok(Meld {
        step: 0,
        seat: seat,
        type_: meld_type,
        tiles: tiles,
        froms: froms,
    })
}

fn wind_from_char(c: char) -> Result<Index, String> {
    Ok(match c {
        'E' => 1,
        'S' => 2,
        'W' => 3,
        'N' => 4,
        _ => return Err(format!("invalid wind symbol: {}", c)),
    })
}

fn print_usage() {
    error!(
        r"invalid input
Usage
    $ cargo run C EXPRESSION [-d]
    $ cargo run C -f FILE [-d]
Options
    -d: print debug info
    -f: read expresisons from file instead of a commandline expression
"
    );
}
