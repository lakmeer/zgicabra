
//
// Stateless ASCII compressor meter -- one line, 23-cell bar plus a
// peak-held GR readout. All the ballistics (attack/release smoothing, GR
// peak-hold/decay) already happen audio-side in Crusher (see
// audio/crusher.rs's disp_env_db/disp_out_db/gr_peak_db) and get relayed
// out via ReeseVoice's crush_env_live/crush_out_live/crush_gr_live Shared
// cells, so this just draws whatever those cells hold right now -- same
// pattern as every other `_live` telemetry field this panel already draws.
//
// Input vs. output level share one dB bar (a la FabFilter Pro-C): solid
// fill up to output level, a fainter shade filling the gap up to input
// level -- that gap *is* the gain reduction. An underlined cell marks the
// live threshold. Trailing number is the peak-held GR in dB.
//

use rgb::RGB8;
use termion::style::{Underline, NoUnderline};

use super::utils::{fg, GREEN_0, RED_0, FG_RESET};

const CELLS:   usize = 23;
const MIN_DB:  f32 = -46.0;
const MAX_DB:  f32 = 0.0;
const STEP_DB: f32 = (MAX_DB - MIN_DB) / (CELLS as f32 - 1.0);

const YELLOW:     RGB8 = RGB8 { r: 235, g: 200, b: 90  };
const ORANGE:     RGB8 = RGB8 { r: 235, g: 150, b: 60  };
const GRAY:       RGB8 = RGB8 { r: 90,  g: 90,  b: 90  };
const NEAR_WHITE: RGB8 = RGB8 { r: 220, g: 220, b: 220 };

pub fn render_compressor_meter (input_db: f32, output_db: f32, threshold_db: f32, gr_peak_db: f32) -> String {
    let mut s = String::from("[");
    let threshold_cell = db_to_cell(threshold_db);

    for i in 0..CELLS {
        let cell_db = MIN_DB + i as f32 * STEP_DB;
        let (ch, color) = if cell_db <= output_db {
            ('\u{2588}', zone_color(cell_db)) // █
        } else if cell_db <= input_db {
            ('\u{2593}', ORANGE) // ▓ -- shaved off by the compressor right now
        } else {
            ('\u{00b7}', GRAY) // · -- dim headroom
        };

        if i == threshold_cell {
            s += &format!("{}{}{}{}{}", Underline, fg(color), ch, FG_RESET, NoUnderline);
        } else {
            s += &format!("{}{}{}", fg(color), ch, FG_RESET);
        }
    }
    s.push(']');

    s += &format!(" {}{}", fg(gr_readout_color(gr_peak_db)), FG_RESET);
    s
}

fn db_to_cell (db: f32) -> usize {
    (((db - MIN_DB) / STEP_DB).round() as isize).clamp(0, CELLS as isize - 1) as usize
}

fn zone_color (db: f32) -> RGB8 {
    if db > -6.0 { RED_0 }
    else if db > -18.0 { YELLOW }
    else { GREEN_0 }
}

fn gr_readout_color (gr_db: f32) -> RGB8 {
    match gr_db {
        g if g < 1.0  => NEAR_WHITE,
        g if g < 6.0  => YELLOW,
        g if g < 12.0 => ORANGE,
        _             => RED_0,
    }
}
