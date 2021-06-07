use super::*;
use crate::util::common::vec_to_string;

use TileStateType::*;

pub const Z8: Tile = Tile(TZ, UK); // unknown tile
pub type TileRow = [usize; TNUM];
pub type TileTable = [TileRow; TYPE];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "t", content = "c")]
pub enum TileStateType {
    H(Seat),        // Hand
    M(Seat, Index), // Meld
    K(Seat, Index), // Kita
    D(Seat, Index), // Discard
    R,              // doRa
    U,              // Unknown
}

impl Default for TileStateType {
    fn default() -> Self {
        U
    }
}

impl fmt::Display for TileStateType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            H(s) => write!(f, "H{}", s),
            M(s, _) => write!(f, "M{}", s),
            K(s, _) => write!(f, "K{}", s),
            D(s, _) => write!(f, "D{}", s),
            R => write!(f, "R "),
            U => write!(f, "U "),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum DrawType {
    Kyushukyuhai,   // 九種九牌
    Suufuurenda,    // 四風連打
    Suukansanra,    // 四槓散了
    Suuchariichi,   // 四家立直
    Kouhaiheikyoku, // 荒廃平局
}

#[derive(Debug, Default, Serialize)]
pub struct Stage {
    pub round: usize,                            // 場 (東:0, 南:1, 西:2, 北:3)
    pub kyoku: usize,                            // 局 (0~3 = 親のseat)
    pub honba: usize,                            // 本場
    pub kyoutaku: usize,                         // リーチ棒の供託
    pub turn: Seat,                              // 順番
    pub step: usize,                             // ステップ op関数を呼び出す毎に+1する
    pub left_tile_count: usize,                  // 牌山の残り枚数
    pub doras: Vec<Tile>,                        // ドラ表示牌
    pub discards: Vec<(Seat, Index)>,            // プレイヤー全員の捨て牌
    pub last_tile: Option<(Seat, OpType, Tile)>, // 他家にロンされる可能性のある牌(捨て牌,槍槓) フリテン判定用
    pub last_riichi: Option<Seat>,               // リーチがロンされずに成立した場合の供託更新用
    pub players: [Player; SEAT],                 // 各プレイヤー情報
    pub is_3p: bool,                             // 三麻フラグ(未実装, 常にfalse)
    pub tile_states: [[[TileStateType; TILE]; TNUM]; TYPE],
    pub tile_remains: [[usize; TNUM]; TYPE], // 牌の残り枚数 = 山+手牌(捨て牌,副露牌,ドラ表示牌以外)
}

impl Stage {
    #[inline]
    pub fn is_leader(&self, seat: Seat) -> bool {
        seat == self.kyoku
    }

    #[inline]
    pub fn get_prevalent_wind(&self) -> Tnum {
        self.round % SEAT + 1 // WE | WS | WW | WN
    }

    #[inline]
    pub fn get_seat_wind(&self, seat: Seat) -> Tnum {
        (seat + SEAT - self.kyoku) % SEAT + 1 // WE | WS | WW | WN
    }

    pub fn get_scores(&self) -> [Score; SEAT] {
        let mut scores = [0; SEAT];
        for s in 0..SEAT {
            scores[s] = self.players[s].score;
        }
        scores
    }
}

impl fmt::Display for Stage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "round: {}, hand: {}, honba: {}, kyoutaku: {}\n\
            turn: {}, left_tile_count: {}, doras: {}, last_tile: {:?}",
            self.round,
            self.kyoku,
            self.honba,
            self.kyoutaku,
            self.turn,
            self.left_tile_count,
            vec_to_string(&self.doras),
            self.last_tile,
        )?;
        writeln!(f)?;

        writeln!(f, "--------------------------------------------------")?;
        for p in &self.players {
            writeln!(f, "{}", p)?;
            writeln!(f, "--------------------------------------------------")?;
        }
        writeln!(f, "")?;

        for ti in 0..TYPE {
            for i in 1..TNUM {
                write!(f, "{}{} ", ['m', 'p', 's', 'z'][ti], i)?;
            }
            writeln!(f, "")?;
            writeln!(f, "--------------------------")?;
            for pi in 0..TILE {
                for i in 1..TNUM {
                    write!(f, "{} ", self.tile_states[ti][i][pi])?;
                }
                writeln!(f, "")?;
            }
            writeln!(f, "")?;
        }

        writeln!(f, "remaining tiles")?;
        for ti in 0..TYPE {
            writeln!(
                f,
                "{}: {:?}",
                ['m', 'p', 's', 'z'][ti],
                self.tile_remains[ti]
            )?;
        }
        writeln!(f, "")
    }
}