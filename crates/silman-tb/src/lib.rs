//! Syzygy endgame tablebase probing for silman, via the vendored
//! [Fathom](https://github.com/jdart1/Fathom) C library (MIT).
//!
//! Fathom keeps all tablebase state in process-wide globals, so at most one
//! [`Tablebase`] may exist per process at a time. [`Tablebase::init`] enforces
//! this with a global lock; dropping the `Tablebase` frees the C-side state
//! and allows a new `init`.
//!
//! WDL probes ([`Tablebase::probe_wdl`] / [`Tablebase::probe_board`]) are
//! thread-safe and take `&self`. Root DTZ probes ([`Tablebase::probe_root`] /
//! [`Tablebase::probe_root_board`]) are *not* thread-safe in Fathom and
//! therefore take `&mut self`.
//!
//! Positions with castling rights cannot be probed (Syzygy tables only cover
//! castling-free positions), and WDL probes additionally require a zero
//! 50-move counter — both restrictions mirror Fathom's own `tb_probe_wdl` /
//! `tb_probe_root` wrapper macros, whose logic is replicated in Rust here
//! (see `ffi` module docs for the ABI provenance).

use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use cozy_chess::{Board, Color, Piece, Rank, Square};

/// Hand-written declarations for the vendored Fathom C library.
///
/// ABI matches Fathom commit `c9c6fef0dddc05d2e242c183acf5833149ab676d`
/// (master, 2025) of <https://github.com/jdart1/Fathom>, vendored in
/// `vendor/fathom/`. Upstream's `tb_probe_wdl` and `tb_probe_root` are
/// `static inline` wrappers in `tbprobe.h` over the exported `*_impl`
/// functions; the wrapper logic (reject nonzero castling rights, and for WDL
/// probes a nonzero 50-move counter) is replicated in safe Rust in
/// [`Tablebase`].
mod ffi {
    use std::os::raw::{c_char, c_uint};

    pub const TB_RESULT_FAILED: c_uint = 0xFFFF_FFFF;
    /// `TB_SET_WDL(0, TB_WIN)` — sentinel for "side to move is checkmated".
    pub const TB_RESULT_CHECKMATE: c_uint = 4;
    /// `TB_SET_WDL(0, TB_DRAW)` — sentinel for "side to move is stalemated".
    pub const TB_RESULT_STALEMATE: c_uint = 2;

    pub const TB_RESULT_WDL_MASK: c_uint = 0x0000_000F;
    pub const TB_RESULT_TO_MASK: c_uint = 0x0000_03F0;
    pub const TB_RESULT_FROM_MASK: c_uint = 0x0000_FC00;
    pub const TB_RESULT_PROMOTES_MASK: c_uint = 0x0007_0000;
    pub const TB_RESULT_EP_MASK: c_uint = 0x0008_0000;
    pub const TB_RESULT_DTZ_MASK: c_uint = 0xFFF0_0000;
    pub const TB_RESULT_WDL_SHIFT: u32 = 0;
    pub const TB_RESULT_TO_SHIFT: u32 = 4;
    pub const TB_RESULT_FROM_SHIFT: u32 = 10;
    pub const TB_RESULT_PROMOTES_SHIFT: u32 = 16;
    pub const TB_RESULT_EP_SHIFT: u32 = 19;
    pub const TB_RESULT_DTZ_SHIFT: u32 = 20;

    extern "C" {
        /// Largest piece count covered by the loaded tables (0 if none).
        /// Written by `tb_init`; read-only from Rust.
        pub static mut TB_LARGEST: c_uint;

        pub fn tb_init(path: *const c_char) -> bool;
        pub fn tb_free();
        pub fn tb_probe_wdl_impl(
            white: u64,
            black: u64,
            kings: u64,
            queens: u64,
            rooks: u64,
            bishops: u64,
            knights: u64,
            pawns: u64,
            ep: c_uint,
            turn: bool,
        ) -> c_uint;
        pub fn tb_probe_root_impl(
            white: u64,
            black: u64,
            kings: u64,
            queens: u64,
            rooks: u64,
            bishops: u64,
            knights: u64,
            pawns: u64,
            rule50: c_uint,
            ep: c_uint,
            turn: bool,
            results: *mut c_uint,
        ) -> c_uint;
    }
}

/// Win/Draw/Loss value from the perspective of the side to move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Wdl {
    /// Side to move loses.
    Loss,
    /// Side to move loses, but the 50-move rule saves the game (draw).
    BlessedLoss,
    Draw,
    /// Side to move wins, but the 50-move rule spoils it (draw).
    CursedWin,
    /// Side to move wins.
    Win,
}

impl Wdl {
    fn from_raw(raw: u32) -> Result<Self, TbError> {
        Ok(match raw {
            0 => Wdl::Loss,
            1 => Wdl::BlessedLoss,
            2 => Wdl::Draw,
            3 => Wdl::CursedWin,
            4 => Wdl::Win,
            _ => return Err(TbError::ProbeFailed),
        })
    }
}

/// Errors from tablebase initialization and probing.
#[derive(Debug, thiserror::Error)]
pub enum TbError {
    /// Fathom uses process-global state; only one [`Tablebase`] may exist at a
    /// time. Drop the existing one first.
    #[error("a Tablebase is already initialized in this process")]
    AlreadyInitialized,
    /// The tablebase directory path could not be passed to C (non-UTF-8 or
    /// contains an interior NUL byte).
    #[error("tablebase path {0:?} cannot be converted for the C API")]
    InvalidPath(PathBuf),
    /// `tb_init` itself failed.
    #[error("tablebase initialization failed for {0:?}")]
    InitFailed(PathBuf),
    /// `tb_init` succeeded but found no tablebase files in the directory.
    #[error("no Syzygy tablebase files found in {0:?}")]
    NoTables(PathBuf),
    /// Syzygy tables only cover positions without castling rights.
    #[error("position has castling rights; Syzygy tables require castling rights to be zero")]
    CastlingRights,
    /// Fathom's WDL probe requires the 50-move counter to be zero.
    #[error("WDL probe requires a zero 50-move counter, got {0}")]
    NonzeroRule50(u32),
    /// More pieces on the board than the largest loaded table covers.
    #[error("position has {count} pieces but the largest loaded table covers {largest}")]
    TooManyPieces { count: u32, largest: u32 },
    /// The C probe returned `TB_RESULT_FAILED` (e.g. a required table file is
    /// missing, or the position is invalid).
    #[error("tablebase probe failed (missing table file or invalid position)")]
    ProbeFailed,
}

/// Position input for a probe, in Fathom's native representation: one bitboard
/// per color and per piece type (bit `1 << square`, a1 = 0 … h8 = 63, i.e.
/// `rank * 8 + file` — the same indexing `cozy_chess` uses).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PositionParams {
    /// All white pieces.
    pub white: u64,
    /// All black pieces.
    pub black: u64,
    pub kings: u64,
    pub queens: u64,
    pub rooks: u64,
    pub bishops: u64,
    pub knights: u64,
    pub pawns: u64,
    /// 50-move-rule halfmove counter. Must be 0 for WDL probes.
    pub rule50: u32,
    /// Castling rights bitmask (`TB_CASTLING_*`). Must be 0 to probe.
    pub castling: u32,
    /// En passant capture-target square index, or 0 if none.
    pub ep: u32,
    /// `true` = white to move.
    pub white_to_move: bool,
}

impl PositionParams {
    /// Extract probe parameters from a [`cozy_chess::Board`].
    pub fn from_board(board: &Board) -> Self {
        let mut castling = 0u32;
        let w = board.castle_rights(Color::White);
        let b = board.castle_rights(Color::Black);
        if w.short.is_some() {
            castling |= 0x1; // TB_CASTLING_K
        }
        if w.long.is_some() {
            castling |= 0x2; // TB_CASTLING_Q
        }
        if b.short.is_some() {
            castling |= 0x4; // TB_CASTLING_k
        }
        if b.long.is_some() {
            castling |= 0x8; // TB_CASTLING_q
        }

        // cozy_chess reports the en passant *file*; Fathom wants the capture
        // target square (rank 6 when white is to move, rank 3 when black is).
        let ep = match board.en_passant() {
            Some(file) => {
                let rank = match board.side_to_move() {
                    Color::White => Rank::Sixth,
                    Color::Black => Rank::Third,
                };
                Square::new(file, rank) as u32
            }
            None => 0,
        };

        PositionParams {
            white: board.colors(Color::White).0,
            black: board.colors(Color::Black).0,
            kings: board.pieces(Piece::King).0,
            queens: board.pieces(Piece::Queen).0,
            rooks: board.pieces(Piece::Rook).0,
            bishops: board.pieces(Piece::Bishop).0,
            knights: board.pieces(Piece::Knight).0,
            pawns: board.pieces(Piece::Pawn).0,
            rule50: u32::from(board.halfmove_clock()),
            castling,
            ep,
            white_to_move: board.side_to_move() == Color::White,
        }
    }

    fn piece_count(&self) -> u32 {
        (self.white | self.black).count_ones()
    }

    fn bare_kings(&self) -> bool {
        (self.white | self.black) == self.kings
    }
}

/// Result of a root DTZ probe ([`Tablebase::probe_root`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootProbe {
    /// The side to move is checkmated.
    Checkmate,
    /// The side to move is stalemated.
    Stalemate,
    /// A tablebase result with a WDL-preserving suggested move.
    Move(RootMove),
}

/// A suggested root move with its WDL and DTZ values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RootMove {
    /// WDL from the perspective of the side to move.
    pub wdl: Wdl,
    pub from: Square,
    pub to: Square,
    /// Promotion piece, if the suggested move is a promotion.
    pub promotion: Option<Piece>,
    /// `true` if the suggested move is an en passant capture.
    pub en_passant: bool,
    /// Distance to zeroing (of the 50-move counter).
    pub dtz: u32,
}

/// Guards Fathom's process-global state: `true` while a [`Tablebase`] exists.
static TB_GUARD: Mutex<bool> = Mutex::new(false);

/// Handle to the process-wide Syzygy tablebase state.
///
/// See the crate docs for the threading and lifetime rules.
#[derive(Debug)]
pub struct Tablebase {
    largest: u32,
}

impl Tablebase {
    /// Initialize the tablebase from a directory of `.rtbw`/`.rtbz` files.
    ///
    /// Fathom keeps its state in globals and `tb_init` is not safe to run
    /// concurrently or twice, so this is guarded by a process-global lock:
    /// a second call fails with [`TbError::AlreadyInitialized`] until the
    /// first `Tablebase` is dropped.
    pub fn init(path: &Path) -> Result<Self, TbError> {
        let mut initialized = TB_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        if *initialized {
            return Err(TbError::AlreadyInitialized);
        }

        let c_path = path
            .to_str()
            .and_then(|s| CString::new(s).ok())
            .ok_or_else(|| TbError::InvalidPath(path.to_owned()))?;

        // Safety: the guard above ensures no other Fathom state exists and no
        // probes can be running (probes require a live `Tablebase`).
        let ok = unsafe { ffi::tb_init(c_path.as_ptr()) };
        if !ok {
            return Err(TbError::InitFailed(path.to_owned()));
        }
        // Safety: TB_LARGEST is only written by tb_init/tb_free, both of which
        // run under TB_GUARD; plain read of an unsigned int.
        let largest = unsafe { std::ptr::addr_of!(ffi::TB_LARGEST).read() };
        if largest == 0 {
            // tb_init reports success even when the directory holds no tables.
            unsafe { ffi::tb_free() };
            return Err(TbError::NoTables(path.to_owned()));
        }

        *initialized = true;
        Ok(Tablebase { largest })
    }

    /// Largest piece count covered by the loaded tables (Fathom's
    /// `TB_LARGEST`), e.g. 5 for a full 3-4-5 set.
    pub fn largest(&self) -> u32 {
        self.largest
    }

    /// Probe the WDL tables. Thread-safe.
    ///
    /// Requirements (mirroring Fathom's `tb_probe_wdl` wrapper): no castling
    /// rights, 50-move counter zero, and at most [`largest`](Self::largest)
    /// pieces. A bare-kings position is answered as [`Wdl::Draw`] without
    /// touching the C library, since Syzygy sets contain no 2-man table.
    pub fn probe_wdl(&self, pos: &PositionParams) -> Result<Wdl, TbError> {
        if pos.castling != 0 {
            return Err(TbError::CastlingRights);
        }
        if pos.rule50 != 0 {
            return Err(TbError::NonzeroRule50(pos.rule50));
        }
        if pos.bare_kings() {
            return Ok(Wdl::Draw);
        }
        let count = pos.piece_count();
        if count > self.largest {
            return Err(TbError::TooManyPieces {
                count,
                largest: self.largest,
            });
        }
        // Safety: tables are initialized (self exists) and stay alive for the
        // duration of the call; WDL probing is thread-safe in Fathom.
        let raw = unsafe {
            ffi::tb_probe_wdl_impl(
                pos.white,
                pos.black,
                pos.kings,
                pos.queens,
                pos.rooks,
                pos.bishops,
                pos.knights,
                pos.pawns,
                pos.ep,
                pos.white_to_move,
            )
        };
        if raw == ffi::TB_RESULT_FAILED {
            return Err(TbError::ProbeFailed);
        }
        Wdl::from_raw(raw)
    }

    /// Convenience: [`probe_wdl`](Self::probe_wdl) for a
    /// [`cozy_chess::Board`].
    pub fn probe_board(&self, board: &Board) -> Result<Wdl, TbError> {
        self.probe_wdl(&PositionParams::from_board(board))
    }

    /// Probe the DTZ tables at the root, returning a WDL-preserving suggested
    /// move. NOT thread-safe in Fathom, hence `&mut self`.
    ///
    /// Requires no castling rights and at most [`largest`](Self::largest)
    /// pieces; a nonzero 50-move counter is allowed here. Needs both `.rtbw`
    /// and `.rtbz` files for the position's material.
    pub fn probe_root(&mut self, pos: &PositionParams) -> Result<RootProbe, TbError> {
        if pos.castling != 0 {
            return Err(TbError::CastlingRights);
        }
        let count = pos.piece_count();
        if count > self.largest {
            return Err(TbError::TooManyPieces {
                count,
                largest: self.largest,
            });
        }
        // Safety: tables are initialized; `&mut self` upholds Fathom's
        // requirement that root probes never run concurrently.
        let raw = unsafe {
            ffi::tb_probe_root_impl(
                pos.white,
                pos.black,
                pos.kings,
                pos.queens,
                pos.rooks,
                pos.bishops,
                pos.knights,
                pos.pawns,
                pos.rule50,
                pos.ep,
                pos.white_to_move,
                std::ptr::null_mut(),
            )
        };
        match raw {
            ffi::TB_RESULT_FAILED => Err(TbError::ProbeFailed),
            ffi::TB_RESULT_CHECKMATE => Ok(RootProbe::Checkmate),
            ffi::TB_RESULT_STALEMATE => Ok(RootProbe::Stalemate),
            _ => {
                let wdl =
                    Wdl::from_raw((raw & ffi::TB_RESULT_WDL_MASK) >> ffi::TB_RESULT_WDL_SHIFT)?;
                let from = (raw & ffi::TB_RESULT_FROM_MASK) >> ffi::TB_RESULT_FROM_SHIFT;
                let to = (raw & ffi::TB_RESULT_TO_MASK) >> ffi::TB_RESULT_TO_SHIFT;
                let promotes =
                    (raw & ffi::TB_RESULT_PROMOTES_MASK) >> ffi::TB_RESULT_PROMOTES_SHIFT;
                let en_passant = (raw & ffi::TB_RESULT_EP_MASK) >> ffi::TB_RESULT_EP_SHIFT != 0;
                let dtz = (raw & ffi::TB_RESULT_DTZ_MASK) >> ffi::TB_RESULT_DTZ_SHIFT;
                let promotion = match promotes {
                    0 => None, // TB_PROMOTES_NONE
                    1 => Some(Piece::Queen),
                    2 => Some(Piece::Rook),
                    3 => Some(Piece::Bishop),
                    4 => Some(Piece::Knight),
                    _ => return Err(TbError::ProbeFailed),
                };
                let (Some(from), Some(to)) = (
                    Square::try_index(from as usize),
                    Square::try_index(to as usize),
                ) else {
                    return Err(TbError::ProbeFailed);
                };
                Ok(RootProbe::Move(RootMove {
                    wdl,
                    from,
                    to,
                    promotion,
                    en_passant,
                    dtz,
                }))
            }
        }
    }

    /// Convenience: [`probe_root`](Self::probe_root) for a
    /// [`cozy_chess::Board`].
    pub fn probe_root_board(&mut self, board: &Board) -> Result<RootProbe, TbError> {
        self.probe_root(&PositionParams::from_board(board))
    }
}

impl Drop for Tablebase {
    fn drop(&mut self) {
        let mut initialized = TB_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        // Safety: probes borrow self, so none can be in flight during drop;
        // the guard serializes against a concurrent `init`.
        unsafe { ffi::tb_free() };
        *initialized = false;
    }
}
