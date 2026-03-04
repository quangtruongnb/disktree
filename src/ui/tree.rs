use crate::scanner::{DirEntry, Flag};
use bytesize::ByteSize;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;
use unicode_width::UnicodeWidthStr;

const BAR_WIDTH: usize = 8;
const SIZE_COL_WIDTH: usize = 8; // right-align size to this many chars
// prefix(2) + gap(2) + bar(8) + "  "(2) + size(8) = 22 reserved cols
const RESERVED_WIDTH: usize = 22;

/// Display column width of a string (CJK chars = 2 cols, others = 1).
fn display_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Truncate `s` to fit within `max_cols` display columns, appending `…` if needed.
fn truncate_to_cols(s: &str, max_cols: usize) -> String {
    if display_width(s) <= max_cols {
        return s.to_string();
    }
    if max_cols == 0 {
        return String::new();
    }
    // Reserve 1 col for the ellipsis (… is 1 col wide)
    let budget = max_cols.saturating_sub(1);
    let mut cols = 0;
    let mut result = String::new();
    for ch in s.chars() {
        let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if cols + w > budget {
            break;
        }
        cols += w;
        result.push(ch);
    }
    result.push('…');
    result
}

pub fn build_list_items(
    entries: &[DirEntry],
    parent_size: u64,
    terminal_width: u16,
) -> Vec<ListItem<'static>> {
    let max_label_cols = (terminal_width as usize).saturating_sub(RESERVED_WIDTH);

    // Pass 1: compute the maximum display-column width of "prefix + name + badge",
    // capped so bar+size always fit on screen.
    let max_label_width = entries
        .iter()
        .map(|e| {
            let prefix = 2usize; // "> " or "  "
            let name = display_width(&e.name);
            let badge = match &e.flag {
                Some(Flag::Cache) => display_width(" [CACHE]"),
                Some(Flag::Brew) => display_width(" [BREW]"),
                None => 0,
            };
            prefix + name + badge
        })
        .max()
        .unwrap_or(0)
        .min(max_label_cols);

    // Pass 2: build items with padding so bar+size columns are vertically aligned
    entries
        .iter()
        .map(|entry| {
            let prefix = if entry.is_dir { "> " } else { "  " };
            let badge_str = match &entry.flag {
                Some(Flag::Cache) => " [CACHE]",
                Some(Flag::Brew) => " [BREW]",
                None => "",
            };

            let badge_cols = display_width(badge_str);
            let prefix_cols = display_width(prefix);
            // Available display columns for the name
            let name_budget = max_label_width
                .saturating_sub(prefix_cols)
                .saturating_sub(badge_cols);
            let name = truncate_to_cols(&entry.name, name_budget);

            let label_cols = prefix_cols + display_width(&name) + badge_cols;
            let padding = " ".repeat(max_label_width.saturating_sub(label_cols) + 2);

            // Size bar
            let bar = if parent_size > 0 {
                let ratio = entry.size as f64 / parent_size as f64;
                let filled = (ratio * BAR_WIDTH as f64).round() as usize;
                let empty = BAR_WIDTH.saturating_sub(filled);
                format!("{}{}", "█".repeat(filled), "░".repeat(empty))
            } else {
                "░".repeat(BAR_WIDTH)
            };

            // Right-aligned size string
            let size_raw = ByteSize::b(entry.size).to_string();
            let size_str = format!("{:>width$}", size_raw, width = SIZE_COL_WIDTH);

            let mut spans: Vec<Span> = vec![
                // Prefix (dir indicator)
                if entry.is_dir {
                    Span::styled(prefix.to_string(), Style::default().fg(Color::Blue))
                } else {
                    Span::raw(prefix.to_string())
                },
                // Name (possibly truncated)
                Span::raw(name),
            ];

            // Badge
            match &entry.flag {
                Some(Flag::Cache) => spans.push(Span::styled(
                    badge_str.to_string(),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )),
                Some(Flag::Brew) => spans.push(Span::styled(
                    badge_str.to_string(),
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                )),
                None => {}
            }

            // Padding + bar + size (all aligned)
            spans.push(Span::raw(padding));
            spans.push(Span::styled(bar, Style::default().fg(Color::DarkGray)));
            spans.push(Span::styled(
                format!("  {}", size_str),
                Style::default().fg(Color::Cyan),
            ));

            ListItem::new(Line::from(spans))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_entry(name: &str, size: u64, is_dir: bool, flag: Option<Flag>) -> DirEntry {
        DirEntry {
            name: name.to_string(),
            path: PathBuf::from(format!("/{name}")),
            size,
            is_dir,
            flag,
            children: vec![],
        }
    }

    #[test]
    fn test_build_items_count() {
        let entries = vec![
            make_entry("a", 100, true, None),
            make_entry("b", 50, false, None),
        ];
        let items = build_list_items(&entries, 150, 120);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_cache_badge_present() {
        let entries = vec![make_entry("Caches", 100, true, Some(Flag::Cache))];
        let items = build_list_items(&entries, 100, 120);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn test_brew_badge_present() {
        let entries = vec![make_entry("homebrew", 100, true, Some(Flag::Brew))];
        let items = build_list_items(&entries, 100, 120);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn test_empty_entries() {
        let items = build_list_items(&[], 0, 120);
        assert!(items.is_empty());
    }

    #[test]
    fn test_bars_start_at_same_column() {
        let entries = vec![
            make_entry("short", 80, true, None),
            make_entry("a-much-longer-name", 20, false, Some(Flag::Cache)),
        ];
        let items = build_list_items(&entries, 100, 120);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_unicode_cjk_alignment() {
        // CJK name "文件夹" = 3 chars but 6 display cols
        // ASCII name "ab" = 2 chars, 2 display cols
        // Both should produce items without panicking, bar must appear for both
        let entries = vec![
            make_entry("文件夹", 80, true, None),
            make_entry("ab", 20, false, None),
        ];
        let items = build_list_items(&entries, 100, 80);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_truncate_to_cols_ascii() {
        let s = truncate_to_cols("hello world", 7);
        assert_eq!(display_width(&s), 7);
        assert!(s.ends_with('…'));
    }

    #[test]
    fn test_truncate_to_cols_cjk() {
        // "你好世界" = 4 chars, 8 cols; truncate to 5 cols -> "你好…" = 2+2+1=5
        let s = truncate_to_cols("你好世界", 5);
        assert_eq!(display_width(&s), 5);
        assert!(s.ends_with('…'));
    }

    #[test]
    fn test_truncate_to_cols_fits() {
        let s = truncate_to_cols("hi", 10);
        assert_eq!(s, "hi");
    }

    #[test]
    fn test_vietnamese_display_width_is_single_col() {
        // Vietnamese uses precomposed Latin+diacritic characters.
        // Each character (Đ, ơ, ô, á, …) occupies exactly 1 terminal column.
        let name = "Đơn xin đi công tác";
        // 19 visible characters, all single-width → 19 display cols
        assert_eq!(display_width(name), 19);
    }

    #[test]
    fn test_vietnamese_alignment_with_ascii() {
        // A Vietnamese name mixed with a short ASCII name: both must render
        // without panicking and produce correct item count.
        let entries = vec![
            make_entry("Đơn xin đi công tác", 80, true, None),
            make_entry("ab", 20, false, None),
        ];
        let items = build_list_items(&entries, 100, 80);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_truncate_to_cols_vietnamese() {
        // Truncate to 10 cols: 9 single-width chars + ellipsis = 10 cols
        let s = truncate_to_cols("Đơn xin đi công tác", 10);
        assert_eq!(display_width(&s), 10);
        assert!(s.ends_with('…'));
    }

    #[test]
    fn test_vietnamese_fits_within_budget() {
        // Short Vietnamese name fits without truncation
        let s = truncate_to_cols("công tác", 20);
        assert_eq!(s, "công tác");
    }
}
