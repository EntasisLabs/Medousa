//! Minimal VT cell grid driven by the pure-Rust `vte` parser.
//!
//! Intentionally small (cursor, scroll, CSI CUP/ED/EL/SGR subset) so TUI terminal
//! panes can attach to workshop shell sessions without Zig / libghostty.

use vte::{Params, Parser, Perform};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub bold: bool,
    pub reverse: bool,
    /// Indexed color 0–15 (ANSI / bright), or None for default.
    pub fg: Option<u8>,
    pub bg: Option<u8>,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            bold: false,
            reverse: false,
            fg: None,
            bg: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct VtGrid {
    cols: usize,
    rows: usize,
    cells: Vec<Cell>,
    cursor_col: usize,
    cursor_row: usize,
    bold: bool,
    reverse: bool,
    fg: Option<u8>,
    bg: Option<u8>,
    scroll_top: usize,
    scroll_bottom: usize,
}

impl VtGrid {
    pub fn new(cols: u16, rows: u16) -> Self {
        let cols = cols.max(2) as usize;
        let rows = rows.max(1) as usize;
        Self {
            cols,
            rows,
            cells: vec![Cell::default(); cols * rows],
            cursor_col: 0,
            cursor_row: 0,
            bold: false,
            reverse: false,
            fg: None,
            bg: None,
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
        }
    }

    pub fn cols(&self) -> u16 {
        self.cols as u16
    }

    pub fn rows(&self) -> u16 {
        self.rows as u16
    }

    pub fn cursor(&self) -> (u16, u16) {
        (self.cursor_col as u16, self.cursor_row as u16)
    }

    pub fn cell_at(&self, col: u16, row: u16) -> Cell {
        let col = col as usize;
        let row = row as usize;
        if col >= self.cols || row >= self.rows {
            return Cell::default();
        }
        self.cells[row * self.cols + col]
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        let cols = cols.max(2) as usize;
        let rows = rows.max(1) as usize;
        if cols == self.cols && rows == self.rows {
            return;
        }
        let mut next = vec![Cell::default(); cols * rows];
        let copy_rows = self.rows.min(rows);
        let copy_cols = self.cols.min(cols);
        for row in 0..copy_rows {
            for col in 0..copy_cols {
                next[row * cols + col] = self.cells[row * self.cols + col];
            }
        }
        self.cols = cols;
        self.rows = rows;
        self.cells = next;
        self.cursor_col = self.cursor_col.min(cols.saturating_sub(1));
        self.cursor_row = self.cursor_row.min(rows.saturating_sub(1));
        self.scroll_top = 0;
        self.scroll_bottom = rows.saturating_sub(1);
    }

    pub fn feed_bytes(&mut self, parser: &mut Parser, bytes: &[u8]) {
        parser.advance(self, bytes);
    }

    fn set_cell(&mut self, col: usize, row: usize, cell: Cell) {
        let idx = row * self.cols + col;
        self.cells[idx] = cell;
    }

    fn clear_cell(&mut self, col: usize, row: usize) {
        self.set_cell(col, row, Cell::default());
    }

    fn put_char(&mut self, ch: char) {
        if self.cursor_col >= self.cols {
            self.carriage_return();
            self.line_feed();
        }
        let col = self.cursor_col;
        let row = self.cursor_row;
        let cell = Cell {
            ch,
            bold: self.bold,
            reverse: self.reverse,
            fg: self.fg,
            bg: self.bg,
        };
        self.set_cell(col, row, cell);
        self.cursor_col += 1;
    }

    fn carriage_return(&mut self) {
        self.cursor_col = 0;
    }

    fn line_feed(&mut self) {
        if self.cursor_row >= self.scroll_bottom {
            self.scroll_up(1);
        } else {
            self.cursor_row += 1;
        }
    }

    fn backspace(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        }
    }

    fn tab(&mut self) {
        let next = ((self.cursor_col / 8) + 1) * 8;
        self.cursor_col = next.min(self.cols.saturating_sub(1));
    }

    fn scroll_up(&mut self, lines: usize) {
        let lines = lines.max(1);
        let top = self.scroll_top;
        let bottom = self.scroll_bottom.min(self.rows.saturating_sub(1));
        if top > bottom {
            return;
        }
        let height = bottom - top + 1;
        if lines >= height {
            for row in top..=bottom {
                for col in 0..self.cols {
                    self.clear_cell(col, row);
                }
            }
            return;
        }
        let cols = self.cols;
        for row in top..=(bottom - lines) {
            for col in 0..cols {
                let src = (row + lines) * cols + col;
                let dst = row * cols + col;
                self.cells[dst] = self.cells[src];
            }
        }
        for row in (bottom + 1 - lines)..=bottom {
            for col in 0..cols {
                self.clear_cell(col, row);
            }
        }
    }

    fn scroll_down(&mut self, lines: usize) {
        let lines = lines.max(1);
        let top = self.scroll_top;
        let bottom = self.scroll_bottom.min(self.rows.saturating_sub(1));
        if top > bottom {
            return;
        }
        let height = bottom - top + 1;
        if lines >= height {
            for row in top..=bottom {
                for col in 0..self.cols {
                    self.clear_cell(col, row);
                }
            }
            return;
        }
        let cols = self.cols;
        for row in ((top + lines)..=bottom).rev() {
            for col in 0..cols {
                let src = (row - lines) * cols + col;
                let dst = row * cols + col;
                self.cells[dst] = self.cells[src];
            }
        }
        for row in top..(top + lines) {
            for col in 0..cols {
                self.clear_cell(col, row);
            }
        }
    }

    fn clamp_cursor(&mut self) {
        if self.cols == 0 || self.rows == 0 {
            self.cursor_col = 0;
            self.cursor_row = 0;
            return;
        }
        self.cursor_col = self.cursor_col.min(self.cols - 1);
        self.cursor_row = self.cursor_row.min(self.rows - 1);
    }

    fn cursor_up(&mut self, n: usize) {
        self.cursor_row = self.cursor_row.saturating_sub(n.max(1));
    }

    fn cursor_down(&mut self, n: usize) {
        self.cursor_row = (self.cursor_row + n.max(1)).min(self.rows.saturating_sub(1));
    }

    fn cursor_forward(&mut self, n: usize) {
        self.cursor_col = (self.cursor_col + n.max(1)).min(self.cols.saturating_sub(1));
    }

    fn cursor_back(&mut self, n: usize) {
        self.cursor_col = self.cursor_col.saturating_sub(n.max(1));
    }

    fn cursor_position(&mut self, row: usize, col: usize) {
        self.cursor_row = row.saturating_sub(1).min(self.rows.saturating_sub(1));
        self.cursor_col = col.saturating_sub(1).min(self.cols.saturating_sub(1));
    }

    fn erase_in_display(&mut self, mode: usize) {
        match mode {
            1 => {
                let cursor_row = self.cursor_row;
                let cursor_col = self.cursor_col;
                let cols = self.cols;
                for row in 0..=cursor_row {
                    let end = if row == cursor_row {
                        cursor_col
                    } else {
                        cols.saturating_sub(1)
                    };
                    for col in 0..=end {
                        if col < cols {
                            self.clear_cell(col, row);
                        }
                    }
                }
            }
            2 | 3 => {
                self.cells.fill(Cell::default());
            }
            _ => {
                // 0: from cursor to end of screen
                let cursor_row = self.cursor_row;
                let cursor_col = self.cursor_col;
                let cols = self.cols;
                let rows = self.rows;
                for row in cursor_row..rows {
                    let start = if row == cursor_row { cursor_col } else { 0 };
                    for col in start..cols {
                        self.clear_cell(col, row);
                    }
                }
            }
        }
    }

    fn erase_in_line(&mut self, mode: usize) {
        let row = self.cursor_row;
        let cols = self.cols;
        match mode {
            1 => {
                let end = self.cursor_col.min(cols.saturating_sub(1));
                for col in 0..=end {
                    self.clear_cell(col, row);
                }
            }
            2 => {
                for col in 0..cols {
                    self.clear_cell(col, row);
                }
            }
            _ => {
                for col in self.cursor_col..cols {
                    self.clear_cell(col, row);
                }
            }
        }
    }

    fn apply_sgr(&mut self, params: &Params) {
        let mut saw_any = false;
        for param in params.iter() {
            saw_any = true;
            let code = param.first().copied().unwrap_or(0);
            match code {
                0 => {
                    self.bold = false;
                    self.reverse = false;
                    self.fg = None;
                    self.bg = None;
                }
                1 => self.bold = true,
                7 => self.reverse = true,
                22 => self.bold = false,
                27 => self.reverse = false,
                30..=37 => self.fg = Some((code - 30) as u8),
                39 => self.fg = None,
                40..=47 => self.bg = Some((code - 40) as u8),
                49 => self.bg = None,
                90..=97 => self.fg = Some((code - 90 + 8) as u8),
                100..=107 => self.bg = Some((code - 100 + 8) as u8),
                _ => {}
            }
        }
        if !saw_any {
            self.bold = false;
            self.reverse = false;
            self.fg = None;
            self.bg = None;
        }
    }

    fn param_or(params: &Params, idx: usize, default: u16) -> u16 {
        params
            .iter()
            .nth(idx)
            .and_then(|p| p.first().copied())
            .filter(|&v| v != 0)
            .unwrap_or(default)
    }
}

impl Perform for VtGrid {
    fn print(&mut self, c: char) {
        self.put_char(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\x08' => self.backspace(),
            b'\t' => self.tab(),
            b'\n' | b'\x0b' | b'\x0c' => self.line_feed(),
            b'\r' => self.carriage_return(),
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        _intermediates: &[u8],
        ignore: bool,
        action: char,
    ) {
        if ignore {
            return;
        }
        match action {
            'A' => self.cursor_up(Self::param_or(params, 0, 1) as usize),
            'B' => self.cursor_down(Self::param_or(params, 0, 1) as usize),
            'C' => self.cursor_forward(Self::param_or(params, 0, 1) as usize),
            'D' => self.cursor_back(Self::param_or(params, 0, 1) as usize),
            'H' | 'f' => {
                let row = Self::param_or(params, 0, 1) as usize;
                let col = Self::param_or(params, 1, 1) as usize;
                self.cursor_position(row, col);
            }
            'J' => {
                let mode = params
                    .iter()
                    .next()
                    .and_then(|p| p.first().copied())
                    .unwrap_or(0) as usize;
                self.erase_in_display(mode);
            }
            'K' => {
                let mode = params
                    .iter()
                    .next()
                    .and_then(|p| p.first().copied())
                    .unwrap_or(0) as usize;
                self.erase_in_line(mode);
            }
            'L' => self.scroll_down(Self::param_or(params, 0, 1) as usize),
            'M' => self.scroll_up(Self::param_or(params, 0, 1) as usize),
            'm' => self.apply_sgr(params),
            'r' => {
                let top = Self::param_or(params, 0, 1) as usize;
                let bottom = params
                    .iter()
                    .nth(1)
                    .and_then(|p| p.first().copied())
                    .map(|v| v as usize)
                    .unwrap_or(self.rows);
                let top = top.saturating_sub(1).min(self.rows.saturating_sub(1));
                let bottom = bottom
                    .saturating_sub(1)
                    .min(self.rows.saturating_sub(1))
                    .max(top);
                self.scroll_top = top;
                self.scroll_bottom = bottom;
            }
            _ => {}
        }
        self.clamp_cursor();
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
        if byte == b'c' {
            // RIS — reset to initial state
            let cols = self.cols as u16;
            let rows = self.rows as u16;
            *self = Self::new(cols, rows);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(grid: &mut VtGrid, bytes: &[u8]) {
        let mut parser = Parser::new();
        grid.feed_bytes(&mut parser, bytes);
    }

    #[test]
    fn prints_and_moves_cursor() {
        let mut grid = VtGrid::new(10, 3);
        feed(&mut grid, b"hi");
        assert_eq!(grid.cell_at(0, 0).ch, 'h');
        assert_eq!(grid.cell_at(1, 0).ch, 'i');
        assert_eq!(grid.cursor(), (2, 0));
        feed(&mut grid, b"\r\n");
        assert_eq!(grid.cursor(), (0, 1));
    }

    #[test]
    fn cup_and_clear() {
        let mut grid = VtGrid::new(8, 4);
        feed(&mut grid, b"abcd\x1b[1;1H\x1b[2J");
        assert_eq!(grid.cell_at(0, 0).ch, ' ');
        assert_eq!(grid.cursor(), (0, 0));
    }

    #[test]
    fn sgr_bold() {
        let mut grid = VtGrid::new(8, 2);
        feed(&mut grid, b"\x1b[1mX\x1b[0mY");
        assert!(grid.cell_at(0, 0).bold);
        assert!(!grid.cell_at(1, 0).bold);
        assert_eq!(grid.cell_at(0, 0).ch, 'X');
        assert_eq!(grid.cell_at(1, 0).ch, 'Y');
    }

    #[test]
    fn sgr_indexed_colors() {
        let mut grid = VtGrid::new(8, 2);
        feed(&mut grid, b"\x1b[31;42mA\x1b[91mB\x1b[0mC");
        assert_eq!(grid.cell_at(0, 0).fg, Some(1));
        assert_eq!(grid.cell_at(0, 0).bg, Some(2));
        assert_eq!(grid.cell_at(1, 0).fg, Some(9));
        assert_eq!(grid.cell_at(1, 0).bg, Some(2));
        assert_eq!(grid.cell_at(2, 0).fg, None);
        assert_eq!(grid.cell_at(2, 0).bg, None);
    }

    #[test]
    fn wraps_and_scrolls() {
        let mut grid = VtGrid::new(4, 2);
        feed(&mut grid, b"12345");
        assert_eq!(grid.cell_at(0, 0).ch, '1');
        assert_eq!(grid.cell_at(0, 1).ch, '5');
        feed(&mut grid, b"\n678");
        // scrolled: row0 should be previous row1 content + more
        assert_eq!(grid.rows(), 2);
        assert_eq!(grid.cell_at(0, 0).ch, '5');
    }
}
