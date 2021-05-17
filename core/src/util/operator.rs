use std::fmt;

use crate::model::*;

use PlayerOperation::*;
use TileStateType::*;

#[derive(Debug, PartialEq, Eq)]
pub enum PlayerOperation {
    Nop, // Turn: ツモ切り(主にリーチ中), Call: 鳴き,ロンのスキップ
    // Turn Operations
    Discard(Vec<Tile>), // 打牌 (配列はチー後に捨てることができない牌)
    Ankan(Vec<Tile>),   // 暗槓
    Kakan(Vec<Tile>),   // 加槓
    Riichi(Vec<Tile>),  // リーチ
    Tsumo,              // ツモ
    Kyushukyuhai,       // 九種九牌
    Kita,               // 北抜き
    // Call Operations
    Chii(Vec<(Tile, Tile)>), // チー (配列は鳴きが可能な組み合わせ 以下同様)
    Pon(Vec<(Tile, Tile)>),  // ポン
    Minkan(Vec<Tile>),       // 明槓
    Ron,                     // ロン
}

// Operator trait
pub trait Operator: OperatorClone + Send {
    fn handle_operation(
        &mut self,
        stage: &Stage,
        seat: Seat,
        operatons: &Vec<PlayerOperation>,
    ) -> PlayerOperation;
    fn debug_string(&self) -> String {
        "Operator".to_string()
    }
}

impl fmt::Debug for dyn Operator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.debug_string())
    }
}

// https://stackoverflow.com/questions/30353462/how-to-clone-a-struct-storing-a-boxed-trait-object
pub trait OperatorClone {
    fn clone_box(&self) -> Box<dyn Operator>;
}

impl<T> OperatorClone for T
where
    T: 'static + Operator + Clone,
{
    fn clone_box(&self) -> Box<dyn Operator> {
        Box::new(self.clone())
    }
}

// NullOperator
#[derive(Clone)]
pub struct NullOperator {}

impl NullOperator {
    pub fn new() -> Self {
        NullOperator {}
    }
}

impl Operator for NullOperator {
    fn handle_operation(
        &mut self,
        _stage: &Stage,
        _seat: Seat,
        _operatons: &Vec<PlayerOperation>,
    ) -> PlayerOperation {
        panic!();
    }

    fn debug_string(&self) -> String {
        "NullOperator".to_string()
    }
}

pub fn get_operation_index(ops: &Vec<PlayerOperation>, op: &PlayerOperation) -> (usize, usize) {
    macro_rules! op_without_arg {
        ($x:pat, $ops:expr) => {
            for (op_idx, op2) in $ops.iter().enumerate() {
                if let $x = op2 {
                    return (op_idx, 0);
                }
            }
        };
    }
    macro_rules! op_with_arg {
        ($x:ident, $ops:expr, $v:expr) => {{
            for (op_idx, op2) in $ops.iter().enumerate() {
                if let $x(v2) = op2 {
                    for (arg_idx, &e) in v2.iter().enumerate() {
                        if e == $v[0] {
                            return (op_idx, arg_idx);
                        }
                    }
                }
            }
        }};
    }
    match op {
        Nop => op_without_arg!(Nop, ops),
        Discard(_) => return (0, 0),
        Ankan(v) => op_with_arg!(Ankan, ops, v),
        Kakan(v) => op_with_arg!(Kakan, ops, v),
        Riichi(v) => op_with_arg!(Riichi, ops, v),
        Tsumo => op_without_arg!(Tsumo, ops),
        Kyushukyuhai => op_without_arg!(Kyushukyuhai, ops),
        Kita => op_without_arg!(Kita, ops),
        Chii(v) => op_with_arg!(Chii, ops, v),
        Pon(v) => op_with_arg!(Pon, ops, v),
        Minkan(v) => op_with_arg!(Minkan, ops, v),
        Ron => op_without_arg!(Ron, ops),
    }
    panic!("Operation '{:?}' not found id '{:?}'", op, ops);
}

pub fn count_left_tile(stage: &Stage, seat: Seat, tile: Tile) -> usize {
    let mut n = 0;
    for &st in &stage.tile_states[tile.0][tile.1] {
        match st {
            U => {
                n += 1;
            }
            H(s) => {
                if s != seat {
                    n += 1;
                }
            }
            _ => {}
        }
    }
    n
}

// Block Info
#[derive(Debug)]
pub struct BlockInfo {
    pub tile: Tile, // ブロックのスタート位置
    pub len: usize, // ブロックの長さ
    pub num: usize, // ブロック内の牌の数
}

impl BlockInfo {
    fn new() -> Self {
        Self {
            tile: Tile(TZ, UK),
            len: 0,
            num: 0,
        }
    }
}

pub fn get_block_info(hand: &TileTable) -> Vec<BlockInfo> {
    let mut vbi = vec![];
    let mut bi = BlockInfo::new();

    // 数牌
    for ti in 0..TZ {
        for ni in 1..TNUM {
            let c = hand[ti][ni];
            if bi.len == 0 {
                if c != 0 {
                    // ブロック始端
                    bi.tile = Tile(ti, ni);
                    bi.len = 1;
                    bi.num = c;
                }
            } else {
                if c == 0 {
                    if bi.tile.1 + bi.len < ni {
                        // ブロック終端
                        vbi.push(bi);
                        bi = BlockInfo::new();
                    }
                } else {
                    // ブロック延長
                    bi.len = ni - bi.tile.1 + 1;
                    bi.num += c;
                }
            }
        }
        if bi.len != 0 {
            vbi.push(bi);
            bi = BlockInfo::new();
        }
    }

    // 字牌
    for ni in 1..=DR {
        let c = hand[TZ][ni];
        if c != 0 {
            vbi.push(BlockInfo {
                tile: Tile(TZ, ni),
                len: 1,
                num: c,
            });
        }
    }

    vbi
}

#[test]
fn test_block() {
    let hand = [
        [0, 0, 0, 0, 1, 1, 0, 0, 1, 1],
        [0, 0, 1, 0, 1, 0, 0, 0, 0, 0],
        [0, 1, 0, 1, 0, 1, 0, 0, 3, 0],
        [0, 2, 0, 3, 0, 0, 3, 3, 0, 0],
    ];

    println!("{:?}", get_block_info(&hand));
}