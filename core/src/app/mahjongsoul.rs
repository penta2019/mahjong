use std::time;

use serde_json::{json, Value};

use crate::actor::create_actor;
use crate::actor::nop::Nop;
use crate::actor::Actor;
use crate::controller::stage_controller::StageController;
use crate::hand::evaluate::WinContext;
use crate::model::*;
use crate::util::common::*;
use crate::util::event_writer::EventWriter;
use crate::util::ws_server::{create_ws_server, SendRecv};

use ActionType::*;

// [App]
pub struct App {
    read_only: bool,
    sleep: bool,
    write_to_file: bool,
    msc_port: u32,
    gui_port: u32,
    actor_name: String,
}

impl App {
    pub fn new(args: Vec<String>) -> Self {
        use std::process::exit;

        let mut app = Self {
            read_only: false,
            sleep: false,
            write_to_file: false,
            msc_port: super::MSC_PORT,
            gui_port: super::GUI_PORT,
            actor_name: "".to_string(),
        };

        let mut it = args.iter();
        while let Some(s) = it.next() {
            match s.as_str() {
                "-r" => app.read_only = true,
                "-s" => app.sleep = true,
                "-w" => app.write_to_file = true,
                "-msc-port" => app.msc_port = next_value(&mut it, "-msc-port"),
                "-gui-port" => app.gui_port = next_value(&mut it, "-gui-port"),
                "-0" => app.actor_name = next_value(&mut it, "-0"),
                opt => {
                    println!("Unknown option: {}", opt);
                    exit(0);
                }
            }
        }

        app
    }

    pub fn run(&mut self) {
        let actor = create_actor(&self.actor_name);
        let mut game = Mahjongsoul::new(self.sleep, self.write_to_file, actor);
        let mut server_msc = create_ws_server(self.msc_port);
        let mut server_gui = create_ws_server(self.gui_port);
        let mut connected = false;

        loop {
            let msg = if let Some((s, r)) = server_msc.lock().unwrap().as_ref() {
                if !connected {
                    connected = true;
                    let msg = r#"{"id": "id_mjaction", "op": "subscribe", "data": "mjaction"}"#;
                    s.send(msg.into()).ok();
                }
                match r.recv() {
                    Ok(m) => m,
                    Err(e) => {
                        println!("[Error] {}", e);
                        continue;
                    }
                }
            } else {
                connected = false;
                continue;
            };

            if let Some(event) = game.apply(&serde_json::from_str(&msg).unwrap()) {
                if !self.read_only {
                    send_to_msc(&mut server_msc, "0", "eval", &event);
                }
            }

            send_to_gui(&mut server_gui, "stage", &json!(&game.get_stage()))
        }
    }
}

// [Mahjongsoul]
#[derive(Debug)]
struct Mahjongsoul {
    ctrl: StageController,
    step: usize,
    seat: usize, // my seat
    events: Vec<Value>,
    random_sleep: bool,
    writer: Option<EventWriter>,
    actor: Box<dyn Actor>,
}

impl Mahjongsoul {
    fn new(random_sleep: bool, write_to_file: bool, actor: Box<dyn Actor>) -> Self {
        // actorは座席0に暫定でセットする
        // 新しい局が開始されて座席が判明した際にスワップする
        let writer = if write_to_file {
            Some(EventWriter::new())
        } else {
            None
        };
        let nop = Box::new(Nop::new());
        let actors: [Box<dyn Actor>; SEAT] =
            [nop.clone(), nop.clone(), nop.clone(), nop.clone()];
        Self {
            ctrl: StageController::new(actors, vec![]),
            step: 0,
            seat: NO_SEAT,
            events: vec![],
            random_sleep: random_sleep,
            writer: writer,
            actor: actor,
        }
    }

    #[inline]
    fn get_stage(&self) -> &Stage {
        return self.ctrl.get_stage();
    }

    #[inline]
    fn event(&mut self, event: Event) {
        self.ctrl.handle_event(&event);
        if let Some(w) = &mut self.writer {
            w.push_event(event)
        }
    }

    fn apply(&mut self, msg: &Value) -> Option<Value> {
        match as_str(&msg["id"]) {
            "id_mjaction" => {
                if msg["type"] == json!("message") {
                    self.apply_event(&msg["data"], false)
                } else if msg["type"] == json!("message_cache") {
                    self.apply_event(&msg["data"], true)
                } else {
                    None
                }
            }
            _ => None, // type: "success"
        }
    }

    fn apply_event(&mut self, event: &Value, is_cache: bool) -> Option<Value> {
        let step = as_usize(&event["step"]);
        let name = as_str(&event["name"]);
        let data = &event["data"];

        if step == 0 {
            if self.seat != NO_SEAT {
                self.ctrl.swap_actor(self.seat, &mut self.actor);
                self.seat = NO_SEAT;
            }
            self.step = 0;
            self.events.clear();
        }

        self.events.push(event.clone());
        if self.seat == NO_SEAT {
            if let Value::Object(act) = &data["operation"] {
                self.seat = as_usize(&act["seat"]);
            }

            if name == "ActionDealTile" {
                if let Value::String(_) = &data["tile"] {
                    self.seat = as_usize(&data["seat"]);
                }
            }

            if self.seat == NO_SEAT {
                return None;
            }

            // seatが確定し時点でactorを設定
            self.actor.set_seat(self.seat);
            self.ctrl.swap_actor(self.seat, &mut self.actor);
        }

        let mut act = None;
        while self.step < self.events.len() {
            let event = self.events[self.step].clone();
            assert!(self.step == as_usize(&event["step"]));

            let data = &event["data"];
            let name = &event["name"];
            let is_last = self.step + 1 == self.events.len();
            if !is_cache && is_last && as_str(name) == "ActionNewRound" {
                sleep_ms(3000);
            }
            match as_str(name) {
                "ActionMJStart" => self.handler_mjstart(data),
                "ActionNewRound" => self.handler_newround(data),
                "ActionDealTile" => self.handler_dealtile(data),
                "ActionDiscardTile" => self.handler_discardtile(data),
                "ActionChiPengGang" => self.handler_chipenggang(data),
                "ActionAnGangAddGang" => self.handler_angangaddgang(data),
                "ActionBabei" => self.handler_babei(data),
                "ActionHule" => self.handler_hule(data),
                "ActionLiuJu" => self.handler_liuju(data),
                "ActionNoTile" => self.handler_notile(data),
                s => panic!("Unknown event {}", s),
            };
            self.step += 1;

            if !is_cache {
                let a = &data["operation"];
                if a != &json!(null) {
                    // self.ctrl.select_actionはstageを更新した直後sleepを挟まずに実行する必要がる
                    act = self.select_action(a);
                }
            }
        }

        act
    }

    fn select_action(&mut self, data: &Value) -> Option<Value> {
        if data["operation_list"] == json!(null) {
            return None;
        }

        let seat = as_usize(&data["seat"]);

        let (acts, idxs) = json_parse_action(data);

        let act = self.ctrl.select_action(seat, &acts);
        let arg_idx = if act.0 == Discard || act.0 == Riichi {
            0
        } else {
            idxs[acts.iter().position(|act2| act2 == &act).unwrap()]
        };

        println!("possible: {:?}", acts);
        println!("selected: {:?}", act);
        println!("");
        flush();

        let start = time::Instant::now();
        let Action(tp, cs) = act;
        let ellapsed = start.elapsed().as_millis();

        let stg = self.get_stage();
        let mut sleep = 1000;
        if self.random_sleep && seat == stg.turn && tp != Tsumo {
            // ツモ・ロン・鳴きのキャンセル以外の操作の場合,ランダムにsleep時間(1 ~ 4秒)を取る
            use rand::distributions::{Bernoulli, Distribution};
            let d = Bernoulli::new(0.1).unwrap();
            let mut c = 0;
            loop {
                if c == 30 || d.sample(&mut rand::thread_rng()) {
                    break;
                }
                sleep += 100;
                c += 1;
            }
        }
        if sleep > ellapsed {
            sleep_ms((sleep - ellapsed) as u64);
        }

        let action = match tp {
            Nop => {
                if stg.turn == seat {
                    let idx = 13 - stg.players[seat].melds.len() * 3;
                    format!("action_dapai({})", idx)
                } else {
                    format!("action_cancel()")
                }
            }
            Discard => {
                let idx = calc_dapai_index(stg, seat, cs[0], false);
                format!("action_dapai({})", idx)
            }
            Ankan => {
                format!("action_gang({})", arg_idx)
            }
            Kakan => {
                format!("action_gang({})", arg_idx)
            }
            Riichi => {
                let idx = calc_dapai_index(stg, seat, cs[0], false);
                format!("action_lizhi({})", idx)
            }
            Tsumo => {
                format!("action_zimo()")
            }
            Kyushukyuhai => {
                format!("action_jiuzhongjiupai()")
            }
            Kita => {
                format!("action_babei()")
            }
            Chi => {
                format!("action_chi({})", arg_idx)
            }
            Pon => {
                format!("action_peng({})", arg_idx)
            }
            Minkan => {
                format!("action_gang({})", arg_idx)
            }
            Ron => {
                format!("action_hu()")
            }
        };
        Some(json!(format!("msc.ui.{}", action)))
    }

    fn update_doras(&mut self, data: &Value) {
        let stg = self.get_stage();
        if let Value::Array(doras) = &data["doras"] {
            if doras.len() > stg.doras.len() {
                let t = tile_from_symbol(as_str(doras.last().unwrap()));
                self.event(Event::dora(t));
            }
        }
    }

    fn handler_mjstart(&mut self, _data: &Value) {
        self.event(Event::game_start());
    }

    fn handler_newround(&mut self, data: &Value) {
        let round = as_usize(&data["chang"]);
        let kyoku = as_usize(&data["ju"]);
        let honba = as_usize(&data["ben"]);
        let kyoutaku = as_usize(&data["liqibang"]);

        let mut doras: Vec<Tile> = Vec::new();
        for ps in as_array(&data["doras"]) {
            doras.push(tile_from_symbol(as_str(ps)));
        }

        let mut scores = [0; SEAT];
        for (s, score) in as_enumerate(&data["scores"]) {
            scores[s] = as_i32(&score);
        }

        let mut hands = [vec![], vec![], vec![], vec![]];
        for s in 0..SEAT {
            let hand = &mut hands[s];
            if s == self.seat {
                for ps in as_array(&data["tiles"]) {
                    hand.push(tile_from_symbol(as_str(ps)));
                }
            } else {
                if s == kyoku {
                    for _ in 0..14 {
                        hand.push(Z8);
                    }
                } else {
                    for _ in 0..13 {
                        hand.push(Z8);
                    }
                }
            }
        }

        self.event(Event::round_new(
            round, kyoku, honba, kyoutaku, doras, scores, hands,
        ));
    }

    fn handler_dealtile(&mut self, data: &Value) {
        self.update_doras(data);
        let s = as_usize(&data["seat"]);

        if let Value::String(ps) = &data["tile"] {
            let t = tile_from_symbol(&ps);
            self.event(Event::deal_tile(s, t));
        } else {
            self.event(Event::deal_tile(s, Z8));
        }
    }

    fn handler_discardtile(&mut self, data: &Value) {
        let s = as_usize(&data["seat"]);
        let t = tile_from_symbol(as_str(&data["tile"]));
        let m = as_bool(&data["moqie"]);
        let r = as_bool(&data["is_liqi"]);
        self.event(Event::discard_tile(s, t, m, r));
        self.update_doras(data);
    }

    fn handler_chipenggang(&mut self, data: &Value) {
        let s = as_usize(&data["seat"]);
        let tp = match as_usize(&data["type"]) {
            0 => MeldType::Chi,
            1 => MeldType::Pon,
            2 => MeldType::Minkan,
            _ => panic!("Unknown meld type"),
        };

        let mut tiles = vec![];
        let mut froms = vec![];
        for ps in as_array(&data["tiles"]) {
            tiles.push(tile_from_symbol(as_str(ps)));
        }
        for f in as_array(&data["froms"]) {
            froms.push(as_usize(f));
        }

        let mut consumed = vec![];
        for (&t, &f) in tiles.iter().zip(froms.iter()) {
            if s == f {
                consumed.push(t);
            }
        }

        self.event(Event::meld(s, tp, consumed));
    }

    fn handler_angangaddgang(&mut self, data: &Value) {
        let s = as_usize(&data["seat"]);
        let tp = match as_usize(&data["type"]) {
            2 => MeldType::Kakan,
            3 => MeldType::Ankan,
            _ => panic!("invalid gang type"),
        };

        let mut t = tile_from_symbol(as_str(&data["tiles"]));
        let consumed = if tp == MeldType::Ankan {
            t = t.to_normal();
            let t0 = if t.is_suit() && t.1 == 5 {
                Tile(t.0, 0)
            } else {
                t
            };
            vec![t, t, t, t0] // t0は数牌の5の場合,赤5になる
        } else {
            vec![t]
        };
        self.event(Event::meld(s, tp, consumed));
    }

    fn handler_babei(&mut self, data: &Value) {
        let s = as_usize(&data["seat"]);
        let m = as_bool(&data["moqie"]);

        self.event(Event::kita(s, m));
    }

    fn handler_hule(&mut self, data: &Value) {
        let mut delta_scores = [0; SEAT];
        for (s, score) in as_enumerate(&data["delta_scores"]) {
            delta_scores[s] = as_i32(score);
        }

        let mut wins = vec![];
        for win in as_array(&data["hules"]) {
            let seat = as_usize(&win["seat"]);
            let count = as_usize(&win["count"]);
            let is_yakuman = as_bool(&win["yiman"]);
            let fan = if is_yakuman { 0 } else { count };
            let yakuman_times = if is_yakuman { count } else { 0 };
            let ctx = WinContext {
                yakus: vec![], // TODO
                n_dora: 0,     // TODO
                n_ura_dora: 0, // TODO
                fu: as_usize(&win["fu"]),
                fan: fan,
                yakuman_times: yakuman_times,
                points: (
                    as_i32(&win["point_rong"]),
                    as_i32(&win["point_zimo_xian"]),
                    win["point_zimo_qin"].as_i64().unwrap_or(0) as Point,
                ),
            };
            wins.push((seat, delta_scores.clone(), ctx));
            delta_scores = [0; SEAT]; // ダブロン,トリロンの場合の内訳は不明なので最初の和了に集約
        }

        self.event(Event::round_end_win(vec![], wins));
    }

    fn handler_liuju(&mut self, _data: &Value) {
        // TODO
        self.event(Event::round_end_draw(DrawType::Kyushukyuhai));
    }

    fn handler_notile(&mut self, data: &Value) {
        let mut points = [0; SEAT];
        if let Some(ds) = &data["scores"][0]["delta_scores"].as_array() {
            for (s, score) in ds.iter().enumerate() {
                points[s] = as_i32(score);
            }
        }

        let mut tenpais = [false; SEAT];
        for (s, player) in as_enumerate(&data["players"]) {
            tenpais[s] = as_bool(&player["tingpai"]);
        }

        self.event(Event::round_end_no_tile(tenpais, points));
    }
}

fn send_to_msc(server: &mut SendRecv, id: &str, op: &str, data: &Value) {
    if let Some((s, _)) = server.lock().unwrap().as_ref() {
        let msg = json!({
            "id": id,
            "op": op,
            "data": data,
        });
        s.send(msg.to_string()).ok();
    }
}

fn send_to_gui(server: &mut SendRecv, type_: &str, data: &Value) {
    if let Some((s, _)) = server.lock().unwrap().as_ref() {
        let msg = json!({
            "type": type_,
            "data": data,
        });
        s.send(msg.to_string()).ok();
    }
}

fn tile_from_symbol(s: &str) -> Tile {
    let b = s.as_bytes();
    let n = b[0] - b'0';
    let t = match b[1] as char {
        'm' => 0,
        'p' => 1,
        's' => 2,
        'z' => 3,
        _ => panic!("invalid Tile type"),
    };
    Tile(t, n as usize)
}

fn calc_dapai_index(stage: &Stage, seat: Seat, tile: Tile, is_drawn: bool) -> usize {
    let pl = &stage.players[seat];
    let h = &pl.hand;
    let t = tile;
    let d = if let Some(d) = pl.drawn { d } else { Z8 };
    let is_drawn = if pl.drawn == Some(t) {
        if pl.hand[t.0][t.1] == 1 || (t.1 == 5 && pl.hand[t.0][5] == 2 && pl.hand[t.0][0] == 1) {
            true
        } else {
            is_drawn
        }
    } else {
        if t.1 == 5 && pl.hand[t.0][t.1] == 1 && Some(Tile(t.0, 0)) == pl.drawn {
            true // ツモった赤5を通常5で指定する場合に通常5がなければ赤5をツモ切り
        } else {
            false
        }
    };

    let mut idx = 0;
    for ti in 0..TYPE {
        for ni in 1..TNUM {
            if h[ti][ni] > 0 {
                if ti == t.0 && ni == t.to_normal().1 && !is_drawn {
                    if ni == 5
                        && h[ti][5] > 1
                        && h[ti][0] == 1
                        && t.1 == 5
                        && pl.drawn != Some(Tile(ti, 0))
                    {
                        return idx + 1; // 赤5が存在しているが指定された牌が通常5の場合
                    } else {
                        return idx;
                    }
                }
                idx += h[ti][ni];
                if ti == d.0 && ni == d.to_normal().1 {
                    idx -= 1;
                }
            }
        }
    }

    if !is_drawn {
        println!("[Error] Tile {} not found", t);
    }

    idx
}

// Actionと元々のデータの各Action内のIndexを返す
fn json_parse_action(v: &Value) -> (Vec<Action>, Vec<Index>) {
    let mut acts = vec![Action::nop()]; // Nop: ツモ切り or スキップ
    let mut idxs = vec![0];
    let mut push = |act: Action, idx: usize| {
        acts.push(act);
        idxs.push(idx);
    };

    for act in as_array(&v["operation_list"]) {
        let combs = &act["combination"];
        match as_i32(&act["type"]) {
            0 => panic!(),
            1 => {
                // 打牌
                let combs = if act["combination"] != json!(null) {
                    json_parse_combination(combs)
                } else {
                    vec![vec![]]
                };
                push(Action(Discard, combs[0].clone()), 0);
            }
            2 => {
                // チー
                for (idx, comb) in json_parse_combination(combs).iter().enumerate() {
                    push(Action::chi(comb.clone()), idx);
                }
            }
            3 => {
                // ポン
                for (idx, comb) in json_parse_combination(combs).iter().enumerate() {
                    push(Action::pon(comb.clone()), idx);
                }
            }
            4 => {
                // 暗槓
                for (idx, comb) in json_parse_combination(combs).iter().enumerate() {
                    push(Action::ankan(comb.clone()), idx);
                }
            }
            5 => {
                // 明槓
                for (idx, comb) in json_parse_combination(combs).iter().enumerate() {
                    push(Action::minkan(comb.clone()), idx);
                }
            }
            6 => {
                // 加槓
                for (idx, comb) in json_parse_combination(combs).iter().enumerate() {
                    push(Action::kakan(comb[0]), idx);
                }
            }
            7 => {
                // リーチ
                for (idx, comb) in json_parse_combination(combs).iter().enumerate() {
                    push(Action::riichi(comb[0]), idx);
                }
            }
            8 => {
                // ツモ
                push(Action::tsumo(), 0);
            }
            9 => {
                // ロン
                push(Action::ron(), 0);
            }
            10 => {
                // 九種九牌
                push(Action::kyushukyuhai(), 0);
            }
            11 => {
                // 北抜き
                push(Action::kita(), 0);
            }
            _ => panic!(),
        }
    }

    (acts, idxs)
}

fn json_parse_combination(combs: &Value) -> Vec<Vec<Tile>> {
    // combsは以下のようなjson list
    // [
    //     "4s|6s",
    //     "6s|7s"
    // ]
    combs
        .as_array()
        .unwrap()
        .iter()
        .map(|comb| {
            let mut c: Vec<Tile> = comb
                .as_str()
                .unwrap()
                .split('|')
                .map(|sym| tile_from_symbol(sym))
                .collect();
            c.sort();
            c
        })
        .collect()
}
